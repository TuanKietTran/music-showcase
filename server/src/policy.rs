//! ABAC policy engine (docs/abac.md section 1-2): default deny, deny overrides, missing attribute = false.
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq)]
pub enum Value { Str(String), Num(f64), Bool(bool), List(Vec<Value>) }
impl From<&str> for Value { fn from(s: &str) -> Value { Value::Str(s.to_string()) } }
impl From<String> for Value { fn from(s: String) -> Value { Value::Str(s) } }
impl From<u64> for Value { fn from(n: u64) -> Value { Value::Num(n as f64) } }
impl From<bool> for Value { fn from(b: bool) -> Value { Value::Bool(b) } }

/// Attribute bag keyed like `subject.role`, `resource.bytes`, `storage.backend`, `env.now`.
pub type Attrs = HashMap<String, Value>;

#[derive(Clone, Debug, PartialEq)]
enum Operand { Lit(Value), Attr(String) }
#[derive(Clone, Copy, Debug, PartialEq)]
enum Op { Eq, Ne, Lt, Le, Gt, Ge, In }
#[derive(Clone, Debug)]
struct Cond { lhs: String, op: Op, rhs: Operand }
#[derive(Clone, Debug)]
pub struct Rule { pub id: String, pub permit: bool, actions: Vec<String>, resource: String, conds: Vec<Cond> }
#[derive(Clone, Debug)]
pub struct Policy { rules: Vec<Rule> }
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decision { Permit, Deny(Option<String>) }

pub const DEFAULT_POLICY: &str = include_str!("default.policy");

fn is_attr_ref(s: &str) -> bool { ["subject.", "resource.", "storage.", "env."].iter().any(|p| s.starts_with(p)) && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_') }

fn parse_scalar(s: &str) -> Result<Value, String> {
    let s = s.trim();
    if let Some(inner) = s.strip_prefix('"') {
        let inner = inner.strip_suffix('"').ok_or("unterminated string")?;
        if inner.contains('"') { return Err("quote inside string".into()); }
        return Ok(Value::Str(inner.to_string()));
    }
    match s { "true" => return Ok(Value::Bool(true)), "false" => return Ok(Value::Bool(false)), _ => {} }
    s.parse::<f64>().map(Value::Num).map_err(|_| format!("bad value '{}'", s))
}

fn parse_operand(s: &str) -> Result<Operand, String> {
    let s = s.trim();
    if s.is_empty() { return Err("missing value".into()); }
    if let Some(inner) = s.strip_prefix('[') {
        let inner = inner.strip_suffix(']').ok_or("unterminated list")?;
        let (mut items, mut cur, mut in_q) = (Vec::new(), String::new(), false);
        for c in inner.chars() {
            match c { '"' => { in_q = !in_q; cur.push(c); } ',' if !in_q => { items.push(std::mem::take(&mut cur)); } _ => cur.push(c) }
        }
        if in_q { return Err("unterminated string".into()); }
        if !cur.trim().is_empty() || !items.is_empty() { items.push(cur); }
        return items.iter().map(|i| parse_scalar(i)).collect::<Result<Vec<_>, _>>().map(|v| Operand::Lit(Value::List(v)));
    }
    if is_attr_ref(s) { return Ok(Operand::Attr(s.to_string())); }
    parse_scalar(s).map(Operand::Lit)
}

fn parse_cond(s: &str) -> Result<Cond, String> {
    let s = s.trim();
    let (lhs, rest) = s.split_once(char::is_whitespace).ok_or("condition needs '<attr> <op> <value>'")?;
    if !is_attr_ref(lhs) { return Err(format!("unknown attribute '{}'", lhs)); }
    let rest = rest.trim_start();
    let (op_s, rhs) = rest.split_once(char::is_whitespace).ok_or("condition needs a value")?;
    let op = match op_s { "==" => Op::Eq, "!=" => Op::Ne, "<" => Op::Lt, "<=" => Op::Le, ">" => Op::Gt, ">=" => Op::Ge, "in" => Op::In, _ => return Err(format!("unknown operator '{}'", op_s)) };
    let rhs = parse_operand(rhs)?;
    if op == Op::In && !matches!(rhs, Operand::Lit(Value::List(_))) { return Err("'in' needs a list".into()); }
    if op != Op::In && matches!(rhs, Operand::Lit(Value::List(_))) { return Err("a list is only valid with 'in'".into()); }
    Ok(Cond { lhs: lhs.to_string(), op, rhs })
}

impl Policy {
    /// Parse the policy text. Errors are `line N: reason` (fatal at startup).
    pub fn parse(text: &str) -> Result<Policy, String> {
        let mut rules: Vec<Rule> = Vec::new();
        for (i, raw) in text.lines().enumerate() {
            let n = i + 1;
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') { continue; }
            let f: Vec<&str> = line.splitn(5, '|').collect();
            if f.len() < 4 { return Err(format!("line {}: expected id|effect|actions|resource|conditions", n)); }
            let id = f[0].trim();
            if id.is_empty() || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') { return Err(format!("line {}: bad rule id", n)); }
            if rules.iter().any(|r| r.id == id) { return Err(format!("line {}: duplicate rule id '{}'", n, id)); }
            let permit = match f[1].trim() { "permit" => true, "deny" => false, e => return Err(format!("line {}: effect must be permit or deny, got '{}'", n, e)) };
            let actions: Vec<String> = f[2].split(',').map(|a| a.trim().to_string()).filter(|a| !a.is_empty()).collect();
            if actions.is_empty() { return Err(format!("line {}: no actions", n)); }
            let resource = f[3].trim().to_string();
            if !["track", "account", "*"].contains(&resource.as_str()) { return Err(format!("line {}: resource must be track, account or *", n)); }
            let mut conds = Vec::new();
            if let Some(c) = f.get(4) {
                for part in c.split(';').filter(|p| !p.trim().is_empty()) { conds.push(parse_cond(part).map_err(|e| format!("line {}: {}", n, e))?); }
            }
            rules.push(Rule { id: id.to_string(), permit, actions, resource, conds });
        }
        Ok(Policy { rules })
    }
    pub fn default_policy() -> Policy { Policy::parse(DEFAULT_POLICY).expect("built-in policy is valid") }
    pub fn rule_count(&self) -> usize { self.rules.len() }

    /// Deny overrides permit; nothing matching = Deny(None).
    pub fn decide(&self, attrs: &Attrs, action: &str, resource_type: &str) -> Decision {
        let mut permitted = false;
        for r in &self.rules {
            if !(r.actions.iter().any(|a| a == "*" || a == action) && (r.resource == "*" || r.resource == resource_type)) { continue; }
            if !r.conds.iter().all(|c| holds(c, attrs)) { continue; }
            if !r.permit { return Decision::Deny(Some(r.id.clone())); }
            permitted = true;
        }
        if permitted { Decision::Permit } else { Decision::Deny(None) }
    }
}

fn holds(c: &Cond, attrs: &Attrs) -> bool {
    let lhs = match attrs.get(&c.lhs) { Some(v) => v, None => return false };
    let rhs_owned;
    let rhs: &Value = match &c.rhs {
        Operand::Lit(v) => v,
        Operand::Attr(a) => match attrs.get(a) { Some(v) => { rhs_owned = v.clone(); &rhs_owned } None => return false },
    };
    match c.op {
        Op::In => match rhs { Value::List(items) => items.iter().any(|i| same_type(lhs, i) && lhs == i), _ => false },
        Op::Eq | Op::Ne => { if !same_type(lhs, rhs) || matches!(lhs, Value::List(_)) { return false; } (lhs == rhs) == (c.op == Op::Eq) }
        _ => match (lhs, rhs) {
            (Value::Num(a), Value::Num(b)) => cmp(c.op, a.partial_cmp(b)),
            (Value::Str(a), Value::Str(b)) => cmp(c.op, Some(a.cmp(b))),
            _ => false,
        },
    }
}
fn cmp(op: Op, o: Option<std::cmp::Ordering>) -> bool {
    use std::cmp::Ordering::*;
    match (op, o) { (Op::Lt, Some(Less)) | (Op::Le, Some(Less | Equal)) | (Op::Gt, Some(Greater)) | (Op::Ge, Some(Greater | Equal)) => true, _ => false }
}
fn same_type(a: &Value, b: &Value) -> bool { std::mem::discriminant(a) == std::mem::discriminant(b) }

#[cfg(test)]
mod tests {
    use super::*;
    fn attrs(pairs: &[(&str, Value)]) -> Attrs { pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect() }
    fn s(v: &str) -> Value { Value::Str(v.into()) }
    fn n(v: u64) -> Value { Value::Num(v as f64) }
    /// Build the attribute bag for an upload decision.
    fn upload(role: &str, status: &str, bytes: u64, remaining: u64, max: u64, backend: &str, encrypted: bool) -> Attrs {
        attrs(&[("subject.id", s("u1")), ("subject.role", s(role)), ("subject.status", s(status)), ("resource.ownerId", s("u1")), ("resource.bytes", n(bytes)),
                ("storage.remainingBytes", n(remaining)), ("storage.maxObjectBytes", n(max)), ("storage.backend", s(backend)), ("storage.encrypted", Value::Bool(encrypted))])
    }
    fn read(role: &str, status: &str, owner: &str, vis: &str) -> Attrs {
        attrs(&[("subject.id", s("u1")), ("subject.role", s(role)), ("subject.status", s(status)), ("resource.ownerId", s(owner)), ("resource.visibility", s(vis))])
    }
    #[test] fn default_policy_parses() { assert_eq!(Policy::default_policy().rule_count(), 8); }
    #[test] fn parser_errors_have_line_numbers() {
        let e = |t: &str| Policy::parse(t).unwrap_err();
        assert!(e("# c\nonly|two|fields").starts_with("line 2"));
        assert!(e("a|allow|*|*|").contains("effect"));
        assert!(e("a|permit|*|thing|").contains("resource"));
        assert!(e("a|permit|*|*|subject.x ~= 1").contains("operator"));
        assert!(e("a|permit|*|*|foo == 1").contains("unknown attribute"));
        assert!(e("a|permit|*|*|subject.x in \"a\"").contains("list"));
        assert!(e("a|permit|*|*|subject.x == [\"a\"]").contains("list"));
        assert!(e("a|permit|*|*|\nb|permit|*|*|subject.x == \"a").starts_with("line 2"));
        assert!(e("a|permit|*|*|\na|deny|*|*|").contains("duplicate"));
        assert!(Policy::parse("a|permit|*|*|").is_ok()); // empty conditions
        assert!(Policy::parse("a|permit|*|*").is_ok()); // conditions field omitted
    }
    #[test] fn default_deny_when_nothing_matches() {
        let p = Policy::parse("x|permit|list|track|subject.role == \"admin\"").unwrap();
        assert_eq!(p.decide(&read("listener", "active", "u1", "private"), "list", "track"), Decision::Deny(None));
    }
    #[test] fn listener_reads_public_and_own_not_others_private() {
        let p = Policy::default_policy();
        assert_eq!(p.decide(&read("listener", "active", "x", "public"), "stream", "track"), Decision::Permit);
        assert_eq!(p.decide(&read("listener", "active", "u1", "private"), "stream", "track"), Decision::Permit);
        assert_eq!(p.decide(&read("listener", "active", "x", "private"), "stream", "track"), Decision::Deny(None));
        assert_eq!(p.decide(&read("listener", "active", "x", "public"), "delete", "track"), Decision::Deny(None));
    }
    #[test] fn admin_can_do_everything_on_active() {
        let p = Policy::default_policy();
        assert_eq!(p.decide(&read("admin", "active", "x", "private"), "delete", "track"), Decision::Permit);
        assert_eq!(p.decide(&read("admin", "active", "x", "private"), "manage", "account"), Decision::Permit);
        assert_eq!(p.decide(&read("uploader", "active", "x", "private"), "manage", "account"), Decision::Deny(None));
    }
    #[test] fn upload_rules() {
        let p = Policy::default_policy();
        assert_eq!(p.decide(&upload("uploader", "active", 10, 100, 1000, "local", false), "upload", "track"), Decision::Permit);
        assert_eq!(p.decide(&upload("listener", "active", 10, 100, 1000, "local", false), "upload", "track"), Decision::Deny(None));
        assert_eq!(p.decide(&upload("uploader", "active", 101, 100, 1000, "local", false), "upload", "track"), Decision::Deny(None)); // quota
        assert_eq!(p.decide(&upload("uploader", "active", 1001, 5000, 1000, "local", false), "upload", "track"), Decision::Deny(Some("deny-oversize".into())));
        assert_eq!(p.decide(&upload("uploader", "active", 10, 100, 1000, "s3", false), "upload", "track"), Decision::Deny(Some("deny-s3-unencrypted".into())));
        assert_eq!(p.decide(&upload("uploader", "active", 10, 100, 1000, "s3", true), "upload", "track"), Decision::Permit);
    }
    #[test] fn deny_overrides_permit_even_for_admin() {
        let p = Policy::default_policy();
        assert_eq!(p.decide(&upload("admin", "suspended", 10, 100, 1000, "local", false), "upload", "track"), Decision::Deny(Some("deny-suspended".into())));
        assert_eq!(p.decide(&read("admin", "suspended", "u1", "public"), "list", "track"), Decision::Deny(Some("deny-suspended".into())));
    }
    #[test] fn missing_attribute_is_false_never_an_error() {
        let p = Policy::parse("a|permit|*|*|subject.nope == \"x\"\nb|deny|*|*|subject.nope != \"x\"").unwrap();
        assert_eq!(p.decide(&Attrs::new(), "list", "track"), Decision::Deny(None));
        let q = Policy::parse("a|permit|*|*|resource.bytes <= storage.remainingBytes").unwrap();
        assert_eq!(q.decide(&attrs(&[("resource.bytes", n(1))]), "x", "track"), Decision::Deny(None)); // rhs missing
    }
    #[test] fn type_mismatch_is_false_and_operators_work() {
        let p = Policy::parse("a|permit|*|*|resource.bytes == \"5\"").unwrap();
        assert_eq!(p.decide(&attrs(&[("resource.bytes", n(5))]), "x", "track"), Decision::Deny(None));
        let q = Policy::parse("a|permit|*|*|resource.bytes >= 5; resource.bytes < 10; subject.role in [\"a\",\"b\"]; env.secure == true").unwrap();
        let ok = attrs(&[("resource.bytes", n(5)), ("subject.role", s("b")), ("env.secure", Value::Bool(true))]);
        assert_eq!(q.decide(&ok, "x", "track"), Decision::Permit);
        let mut bad = ok.clone(); bad.insert("resource.bytes".into(), n(10));
        assert_eq!(q.decide(&bad, "x", "track"), Decision::Deny(None));
    }
    #[test] fn action_and_resource_filters() {
        let p = Policy::parse("a|permit|list,read|track|\nb|permit|manage|account|").unwrap();
        let e = Attrs::new();
        assert_eq!(p.decide(&e, "read", "track"), Decision::Permit);
        assert_eq!(p.decide(&e, "read", "account"), Decision::Deny(None));
        assert_eq!(p.decide(&e, "manage", "account"), Decision::Permit);
        assert_eq!(p.decide(&e, "upload", "track"), Decision::Deny(None));
    }
}
