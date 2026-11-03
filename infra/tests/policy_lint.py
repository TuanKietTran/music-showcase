#!/usr/bin/env python3
"""Offline unit test of the IAM policy templates (no AWS access needed).

Renders infra/policies/*.json.tftpl with sample values, then checks:
  * no Allow statement uses a bare "*" action or a service-wide wildcard such as "s3:*";
  * the writer role can only write/delete under accounts/<its own accountId session tag>/ (tag mismatch = no match);
  * the server (task) role cannot write or delete objects at all, only read accounts/*;
  * the writer is denied outright when the session tag is missing.
Run:  python3 infra/tests/policy_lint.py
"""
import json, pathlib, re, sys

ROOT = pathlib.Path(__file__).resolve().parents[1] / "policies"
VARS = {"bucket_arn": "arn:aws:s3:::media-bucket", "kms_arn": "arn:aws:kms:r:000000000000:key/k", "region": "r",
        "writer_role_arn": "arn:aws:iam::000000000000:role/w", "table_arn": "arn:aws:dynamodb:r:000000000000:table/t",
        "task_role_arn": "arn:aws:iam::000000000000:role/task"}

def render(name):
    text = (ROOT / name).read_text().replace("$${", "\x00")          # tftpl escape for IAM policy variables
    text = re.sub(r"\$\{(\w+)\}", lambda m: VARS[m.group(1)], text)
    return json.loads(text.replace("\x00", "${"))

def as_list(v): return v if isinstance(v, list) else [v]

def glob_match(pattern, value):
    return re.fullmatch("".join(".*" if c == "*" else "." if c == "?" else re.escape(c) for c in pattern), value) is not None

def allowed(policy, action, resource, tags):
    """Tiny IAM evaluator: Allow/Deny statements, wildcards, ${aws:PrincipalTag/x} variables, Null conditions."""
    verdict = False
    for st in policy["Statement"]:
        if not any(glob_match(a, action) for a in as_list(st["Action"])): continue
        pats = [re.sub(r"\$\{aws:PrincipalTag/(\w+)\}", lambda m: tags.get(m.group(1), "\x01unresolved"), r) for r in as_list(st["Resource"])]
        if not any(glob_match(p, resource) for p in pats): continue
        cond = st.get("Condition", {})
        if "Null" in cond:
            if not all((tags.get(k.split("/")[-1]) is None) == (v == "true") for k, v in cond["Null"].items()): continue
        elif cond and not set(cond) <= {"StringEquals"}: continue      # other conditions are checked by AWS, not by this test
        if st["Effect"] == "Deny": return False
        verdict = True
    return verdict

failures = []
def check(ok, msg):
    print(("PASS " if ok else "FAIL ") + msg)
    if not ok: failures.append(msg)

writer, task, trust = render("writer-role.json.tftpl"), render("task-role.json.tftpl"), render("writer-trust.json.tftpl")
B = VARS["bucket_arn"]
for name, pol in (("writer", writer), ("task", task)):
    bad = [a for st in pol["Statement"] if st["Effect"] == "Allow" for a in as_list(st["Action"]) if a == "*" or a.endswith(":*")]
    check(not bad, f"{name} role has no wildcard Allow actions {bad}")
check(allowed(writer, "s3:PutObject", f"{B}/accounts/alice/t1.mp3", {"accountId": "alice"}), "writer: tag alice may put accounts/alice/*")
check(allowed(writer, "s3:DeleteObject", f"{B}/accounts/alice/t1.mp3", {"accountId": "alice"}), "writer: tag alice may delete accounts/alice/*")
check(allowed(writer, "s3:AbortMultipartUpload", f"{B}/accounts/alice/t1.mp3", {"accountId": "alice"}), "writer: tag alice may abort its own multipart upload")
check(not allowed(writer, "s3:PutObject", f"{B}/accounts/bob/t1.mp3", {"accountId": "alice"}), "writer: tag alice is DENIED accounts/bob/* (mismatching tag)")
check(not allowed(writer, "s3:DeleteObject", f"{B}/accounts/bob/t1.mp3", {"accountId": "alice"}), "writer: tag alice cannot delete bob's objects")
check(not allowed(writer, "s3:PutObject", f"{B}/accounts/alice/t1.mp3", {}), "writer: no tag = denied everywhere")
check(not allowed(writer, "s3:GetObject", f"{B}/accounts/alice/t1.mp3", {"accountId": "alice"}), "writer: cannot read (reads belong to the server role)")
check(not allowed(writer, "s3:DeleteObjectVersion", f"{B}/accounts/alice/t1.mp3", {"accountId": "alice"}), "writer: cannot permanently delete versions (restore stays possible)")
check(allowed(task, "s3:GetObject", f"{B}/accounts/anyone/t.mp3", {}), "task: reads accounts/*")
check(not allowed(task, "s3:GetObject", f"{B}/other/t.mp3", {}), "task: cannot read outside accounts/*")
for act in ("s3:PutObject", "s3:DeleteObject", "s3:AbortMultipartUpload"):
    check(not allowed(task, act, f"{B}/accounts/alice/t.mp3", {}), f"task: cannot {act} directly (must go through the tagged writer role)")
tr = trust["Statement"][0]
check("sts:TagSession" in tr["Action"] and tr["Principal"] == {"AWS": VARS["task_role_arn"]}, "writer trust: only the task role, with TagSession")
check(tr["Condition"]["StringLike"] == {"aws:RequestTag/accountId": "?*"}, "writer trust: accountId tag is mandatory")
check(tr["Condition"]["ForAllValues:StringEquals"] == {"aws:TagKeys": ["accountId"]}, "writer trust: no other session tags accepted")
sys.exit(1 if failures else 0)
