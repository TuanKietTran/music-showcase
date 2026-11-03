# Access control (ABAC), accounts, storage and uploads

Status: design contract. Server code, clients and tests follow this file; change this file first.

## 1. Model

Every API call that touches a track or an account is turned into one decision request:

    decide(subject, action, resource, storage, env) -> Permit | Deny(ruleId?)

- **subject**: the authenticated account. Attributes: `id`, `name`, `role` (`admin|uploader|listener`), `status` (`active|suspended`), `quotaBytes`.
- **action**: `list`, `read` (metadata, cover), `stream`, `upload`, `update`, `delete`, `manage` (accounts; admin only in the default policy).
- **resource**: a track (or account for `manage`). Attributes: `type` (`track|account`), `id`, `ownerId`, `visibility` (`public|private`), `format`, `bytes`, `license`, `source`. Built-in tracks have `ownerId` = `system`, `visibility` = `public`. For an upload decision the resource is the *proposed* track: `ownerId` is the subject, `bytes` is the Content-Length.
- **storage**: the backend that holds (or would hold) the bytes. Attributes: `backend` (`local|s3`), `encrypted` (bool; `false` for `local`), `maxObjectBytes`, `usedBytes` (of the resource owner's account), `quotaBytes` (of that account), `remainingBytes` (= quotaBytes - usedBytes, never below 0), `region` and `bucket` (s3 only).
- **env**: `now` (unix seconds), `secure` (true when the request came over TLS or from a trusted proxy header; false for plain local HTTP).

Evaluation: **default deny, deny overrides**. Collect rules whose `actions` and `resource` match; evaluate their conditions; if any matching `deny` rule holds -> Deny(that rule id); else if any `permit` rule holds -> Permit; else Deny(none). A condition that references a missing attribute is false (never an error).

## 2. Policy file

Plain text, one rule per line, `|` separated, `#` comments, blank lines ignored. Loaded from env `MUSIC_POLICY_FILE`; if unset, the built-in default (below, shipped in the binary with `include_str!`) is used. A syntax error is fatal at startup with the line number. The file is read once at start.

    id | effect | actions | resource | conditions

- `effect`: `permit` or `deny`. `actions`: comma list or `*`. `resource`: `track`, `account` or `*`.
- `conditions`: zero or more, separated by `;`, all must hold. Each is `<attr> <op> <value>`.
  - `attr`: `subject.x`, `resource.x`, `storage.x` or `env.x`.
  - `op`: `==`, `!=`, `<`, `<=`, `>`, `>=`, `in`. Numbers compare numerically, everything else as strings/bools; comparing different types is false.
  - `value`: `"string"`, number, `true`, `false`, a list `["a","b"]` (only with `in`), or another attribute reference (e.g. `resource.ownerId == subject.id`, `resource.bytes <= storage.remainingBytes`).

Default policy (built in):

    admin-all|permit|*|*|subject.role == "admin"
    read-public|permit|list,read,stream|track|resource.visibility == "public"
    read-own|permit|list,read,stream|track|resource.ownerId == subject.id
    upload|permit|upload|track|subject.role in ["uploader","admin"]; resource.bytes <= storage.remainingBytes
    edit-own|permit|update,delete|track|resource.ownerId == subject.id
    deny-suspended|deny|*|*|subject.status != "active"
    deny-oversize|deny|upload|track|resource.bytes > storage.maxObjectBytes
    deny-s3-unencrypted|deny|upload|track|storage.backend == "s3"; storage.encrypted != true

Behaviour that follows: listeners can read public tracks and their own; uploaders can additionally upload within quota and edit/delete only their own tracks; admins can do everything including other accounts' tracks; suspended accounts can do nothing; nobody uploads an oversize file or writes unencrypted to s3.

## 3. Accounts

File `MUSIC_DATA_DIR/accounts.tsv` (tab separated, `#` comments): `id`, `name`, `role`, `status`, `quotaBytes`, `tokenSha256` (lowercase hex of SHA-256 of the bearer token). Tokens are never stored in clear. `music-server hash-token` reads a token from stdin and prints its hash (so it never appears in argv or shell history). Lookup hashes the presented token and compares in constant time over all accounts.

Bootstrap: the env `MUSIC_TOKEN` stays required and maps to the built-in account `admin` (role `admin`, status `active`, no quota limit shown as `quotaBytes` 0 = unlimited). Existing clients keep working. Ids are `[a-z0-9_-]{1,64}` (widened from 32 so an OIDC `sub` UUID can be an id verbatim); `system` and `admin` are reserved for the built-ins. A missing accounts file means only the bootstrap admin exists.

## 4. Storage

A storage backend is an object store addressed by keys of the form `accounts/<ownerId>/<trackId>.<ext>`; this layout is also the S3 key layout, so an AWS prefix policy per account works unchanged. Operations: `put(key, body stream, length)`, `open(key)` (range-capable read), `delete(key)`, `stat(key)`, and `attributes()` (the `storage.*` values). The server code uses an enum `Storage { Local(..) }` (no async-trait crate); The `S3` variant is built with `--features aws` (see docs/aws.md). Env `MUSIC_STORAGE` = `local` (default) or `s3`; `s3` is a startup error "s3 backend not available in this build" when the binary was built without the `aws` feature, and also when the bucket's default encryption is not SSE-KMS/SSE-S3. `local` root: `MUSIC_DATA_DIR/objects` (default `./data/objects`), directories 0700, files 0600, writes go to a temp name then `rename`.

Built-in tracks (MUSIC_MEDIA_DIR or synthesized) are read-only and are not stored in the Storage backend.

Index: by default `MUSIC_DATA_DIR/uploads.tsv`; with env `MUSIC_DDB_TABLE` (feature `aws`) the same rows live in a DynamoDB table (`ownerId` partition key, `id` sort key) and the file is not used. Columns: tab separated: `id`, `ownerId`, `visibility`, `title`, `artist`, `key`, `format`, `bytes`, `durationSec`, `createdAt`. Rewritten atomically (temp + rename) under a mutex on every change. Used bytes of an account = sum of its rows.

## 5. API changes

All routes need a bearer token as before; the token now identifies an account. Every denial is decided by `decide`.

- `GET /api/me` -> `200 {id,name,role,status,quotaBytes,usedBytes,maxUploadBytes,can:{upload:bool}}`. `can.upload` is the real `decide` result for a tiny proposed upload, so clients can hide controls.
- `GET /api/tracks` -> as before, but filtered by `decide(list)` for each track; each item gains `ownerId`, `visibility`, `mine` (bool). Uploaded tracks have `sampleRate`/`bitDepth` 0 and `durationSec` 0 when unknown (clients must tolerate 0 duration and take it from the player).
- `POST /api/tracks?title=..&artist=..&visibility=private|public[&durationSec=n]` with the raw file as the body (`Content-Type: application/octet-stream`; the client-claimed type and any filename are ignored). `Content-Length` is required (`411` if missing; chunked is rejected). The server:
  1. authenticates, then `decide(upload)` with `resource.bytes` = Content-Length before reading the body (`403 forbidden` / `413 upload_too_large` / `403 quota_exceeded`);
  2. streams the body into storage under a temp key, never buffering it whole, and stops with `413` if more than Content-Length arrives;
  3. sniffs the first bytes: mp3 (`ID3` or an MPEG frame sync), ogg (`OggS`), flac (`fLaC`), m4a (`ftyp` at offset 4), wav (`RIFF....WAVE`); anything else -> `415 unsupported_media` and the temp object is deleted;
  4. generates the id (16 random hex chars from `/dev/urandom`-quality randomness, e.g. read from the OS), sets `format` from the sniff, sanitizes `title`/`artist` (trim, drop control characters including tab and newline, max 120 chars; title default `Untitled`), renames into place, appends the index row;
  5. answers `201` with the track JSON.
  Defaults: `visibility=private`. Max size env `MUSIC_MAX_UPLOAD_BYTES` (default 104857600).
- `PATCH /api/tracks/{id}?visibility=public|private&title=..&artist=..` -> `200` track JSON; `decide(update)`.
- `DELETE /api/tracks/{id}` -> `204`; `decide(delete)`; removes the object and the index row.
- `GET /api/tracks/{id}`, `/stream`, `/cover`: `decide(read|stream)`. A track the subject may not read answers `404 not_found` (same as a missing id; existence is not leaked). `403` is used only for actions on tracks the subject can already see (update/delete/upload).
- Built-in tracks reject update/delete with `403`.
- Authentication may also be a Cognito/OIDC JWT (feature `aws`, docs/aws.md); it maps to the same subject attributes. A subject-level `deny` (e.g. suspended) answers `403 forbidden` on every track route, not `404`.
- New JSON errors: `{error:"forbidden"}` 403, `{error:"upload_too_large"}` 413, `{error:"quota_exceeded"}` 403, `{error:"unsupported_media"}` 415, `{error:"length_required"}` 411, plus `{error:"upload_failed"}` 500 (storage or index failure; nothing is kept), `{error:"storage_error"}` 500 and `{error:"server_busy"}` 503 (all upload slots in use). The response never names the failing rule; the server logs `ruleId` and account id (never the token) to stderr.
- CORS: allowed methods now `GET, HEAD, POST, PATCH, DELETE, OPTIONS`.
- Cover for an uploaded track: generated SVG as for tracks without art.

## 6. Error UX additions (docs/error-catalog.json, all three clients)

| code | severity | surface | title | message | actions |
| --- | --- | --- | --- | --- | --- |
| forbidden | warning | toast | You can't do that | Your account doesn't have permission for this. | dismiss |
| upload-too-large | warning | inline-banner | That file is too big | Pick a smaller file or trim it; the limit is shown above the picker. | dismiss, retry |
| quota-exceeded | warning | inline-banner | Your storage is full | Delete something you uploaded or ask an admin for more space. | dismiss |
| upload-unsupported | warning | inline-banner | We can't use that file | Upload an MP3, OGG, FLAC, M4A or WAV file. | dismiss, retry |
| upload-failed | error | inline-banner | The upload didn't finish | Your connection may have dropped. Nothing was saved. Try again. | retry, dismiss |

Upload errors are scoped to the upload panel and never replace the library or the player. A failed upload never leaves a partial track in the list.

## 7. Tests the server must have

policy parser (good/bad lines, line numbers), each default rule (permit and deny cases from section 2), deny overrides permit, missing attribute is false, reference-to-attribute comparison, accounts file parser and token hash lookup, magic-byte sniffing for all five formats plus garbage, filename/title sanitizing, upload end-to-end over a real socket (valid mp3 bytes -> 201 -> listed -> streamed with range -> delete -> 404), quota exceeded, listener upload -> 403, private track invisible to another account (404), suspended account -> 403 `forbidden` on every route except `GET /api/me` (which reports `status`).
