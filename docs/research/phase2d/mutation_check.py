"""Deliberate-bug check for the Phase 2d crates (u1, u3, u4), after
docs/research/sam3-headless/mutation_check.py.

Each case breaks one safety rule in the source, runs the one test that should notice, and puts the
source back. A case is CAUGHT only if the mutated code compiled and the test run failed; a case that
does not compile counts as NOT CAUGHT (a bad mutant), so every survivor is either a test gap or a
mutant to fix. Cargo runs serially, targeted with -p, with -j 8.

Usage: mutation_check.py u1|u3|u4 [name-substring]
Saved logs pass through sam3-headless/scrub.py so no Windows account name is written.
"""
import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'docs/research/sam3-headless'))
from scrub import scrub  # noqa: E402

OUT = Path(__file__).resolve().parent / 'verification'
OUT.mkdir(exist_ok=True)
C = 'crates/'
CRLF, LF = '\r\n', '\n'
CONN = C + 'ai-model/src/connection.rs'
CUSTOM = C + 'ai-model/src/draft/custom.rs'
DRAFT = C + 'ai-model/src/draft.rs'
HTTP = C + 'ai-openai/src/http.rs'
PROBE_O = C + 'ai-openai/src/probe.rs'
CATALOG = C + 'ai-openai/src/catalog.rs'
PROBE_C = C + 'ai-client/src/manage/probe.rs'
MANAGE = C + 'ai-client/src/manage.rs'
FACTORY = C + 'ai-client/src/factory.rs'

PACK = C + 'ai-comfy-env/src/source/pack/mod.rs'
PACK_RESOLVE = C + 'ai-comfy-env/src/source/pack/resolve.rs'
PACK_INSTALL = C + 'ai-comfy-env/src/source/pack/install.rs'
PACK_ARCHIVE = C + 'ai-comfy-env/src/source/pack/archive.rs'
HOSTS = C + 'ai-comfy-env/src/source/hosts.rs'
FORMAT = C + 'ai-comfy-env/src/source/format.rs'
SPEC = C + 'ai-comfy-env/src/source/spec.rs'
DOWNLOAD = C + 'ai-comfy-env/src/download.rs'
HF = C + 'ai-comfy-env/src/source/resolve/hf.rs'
GH = C + 'ai-comfy-env/src/source/resolve/github.rs'
CIV = C + 'ai-comfy-env/src/source/resolve/civitai.rs'
API = C + 'ai-comfy-env/src/source/resolve/api.rs'
RESOLVE = C + 'ai-comfy-env/src/source/resolve/mod.rs'
REGISTRY = C + 'ai-comfy-env/src/source/registry.rs'
INSTALL = C + 'ai-comfy-env/src/source/install.rs'

M = 'craft-ai-model'
O = 'craft-ai-openai'
CL = 'craft-ai-client'
# (name, file, old, new, package, test target, test filter)
U1 = [
    ('header-length-unchecked', CONN, 'if name.is_empty() || name.len() > MAX_KEY_HEADER_BYTES {', 'if false {', M, 'custom_connection', 'header_names_that'),
    ('header-length-loosened', CONN, 'pub const MAX_KEY_HEADER_BYTES: usize = 64;', 'pub const MAX_KEY_HEADER_BYTES: usize = 65;', M, 'custom_connection', 'header_names_that'),
    ('header-chars-unrestricted', CONN, ".all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))", '.all(|_| true)', M, 'custom_connection', 'header_names_that'),
    ('proxy-headers-allowed', CONN, 'if lower.starts_with("proxy-") || lower.starts_with("sec-") {', 'if lower.starts_with("sec-") {', M, 'custom_connection', 'header_names_that'),
    ('transport-headers-allowed', CONN, 'if RESERVED_HEADERS.contains(&lower.as_str()) {', 'if false {', M, 'custom_connection', 'header_names_that'),
    ('stored-header-not-validated', CONN, 'check_key_header(name).map_err(ConnectionError::KeyHeader)?;', 'let _ = name;', M, 'custom_connection', 'stored_file'),
    ('stored-header-without-key-allowed', CONN, 'if self.auth.secret_ref().is_none() {\n                return Err(ConnectionError::KeyHeader(', 'if false {\n                return Err(ConnectionError::KeyHeader(', M, 'custom_connection', 'stored_file'),
    ('form-header-without-key-allowed', CUSTOM, 'if !self.uses_key {', 'if false {', M, 'custom_connection', 'without_a_key'),
    ('form-header-not-trimmed', CUSTOM, 'let name = self.key_header.trim();', 'let name = self.key_header.as_str();', M, 'custom_connection', 'named_header_is_kept'),
    ('form-header-dropped', DRAFT, 'enabled: self.enabled,\n            key_header,\n        };', 'enabled: self.enabled,\n            key_header: None,\n        };', M, 'custom_connection', 'named_header_is_kept'),
    ('edit-loses-header', DRAFT, 'key_header: c.key_header.clone().unwrap_or_default(),', 'key_header: String::new(),', M, 'custom_connection', 'named_header_is_kept'),
    ('custom-test-wrong-provider', CUSTOM, 'self.provider.trim() == "openai-compatible"', 'self.provider.trim() == "openai"', M, 'custom_connection', 'starts_from_the_other'),
    ('custom-billing-assumed-free', DRAFT, 'billing: Billing::Unknown,\n        needs_key: true,\n        note: "Any server that speaks the OpenAI API.",', 'billing: Billing::Free,\n        needs_key: true,\n        note: "Any server that speaks the OpenAI API.",', M, 'custom_connection', 'starts_from_the_other'),
    ('named-key-sent-as-bearer', HTTP, 'Some(name) => (name.clone(), key.expose().to_owned()),', 'Some(_) => ("Authorization".to_owned(), format!("Bearer {}", key.expose())),', O, 'custom', 'named_header_carries'),
    ('named-key-value-prefixed', HTTP, 'Some(name) => (name.clone(), key.expose().to_owned()),', 'Some(name) => (name.clone(), format!("Bearer {}", key.expose())),', O, 'custom', 'named_header_carries'),
    ('redirects-followed', HTTP, '.max_redirects(0)', '.max_redirects(5)', O, 'custom', 'redirect_never_carries'),
    ('error-text-not-redacted', HTTP, 'let mut e = scrub(AiError::new(kind, message), key);', 'let mut e = AiError::new(kind, message);', O, 'custom', 'echoes_a_named_key'),
    ('list-cut-disabled', PROBE_O, 'if ids.len() >= MAX_LISTED && !ids.contains(id) {', 'if false {', O, 'custom', 'huge_list'),
    ('list-cut-off-by-one', PROBE_O, 'if ids.len() >= MAX_LISTED && !ids.contains(id) {', 'if ids.len() > MAX_LISTED && !ids.contains(id) {', O, 'custom', 'huge_list'),
    ('list-keeps-unusable-ids', PROBE_O, '.filter(|id| ModelId::new(*id).is_ok());', '.filter(|_| true);', O, 'custom', 'raw_list_keeps'),
    ('list-error-unreadable', PROBE_O, "the server's reply has no model list (expected", "the server's reply is odd (expected", O, 'custom', 'without_a_data_list'),
    ('declared-chat-gets-features', CATALOG, 'Kind::Chat => plain_text_spec(),', 'Kind::Chat => text_spec(),', O, 'custom', 'declared_chat_model'),
    ('declared-image-many-outputs', CATALOG, 'Kind::ImageGenerate => generate_spec(1),', 'Kind::ImageGenerate => generate_spec(10),', O, 'custom', 'image_kinds_map'),
    ('declared-inpaint-without-mask', CATALOG, 'Kind::ImageInpaint => edit_spec("image.inpaint", 1, 0),', 'Kind::ImageInpaint => edit_spec("image.edit", 1, 0),', O, 'custom', 'image_kinds_map'),
    ('declared-nothing-accepted', CATALOG, 'if specs.is_empty() || specs.len() != kinds.len() {', 'if specs.len() != kinds.len() {', O, 'custom', 'nothing_is_declared'),
    ('declared-repeats-kept', CATALOG, 'kinds.sort();\n    kinds.dedup();\n    let specs', 'kinds.sort();\n    let specs', O, 'custom', 'image_kinds_map'),
    ('declared-order-unfixed', CATALOG, 'kinds.sort();\n    kinds.dedup();\n    let specs', 'kinds.dedup();\n    let specs', O, 'custom', 'image_kinds_map'),
    ('control-request-carries-key', PROBE_C, 'match list_models(&config) {', 'match list_models(&keyed) {', CL, 'custom', 'control_request_none'),
    ('key-credited-on-any-failure', PROBE_C, 'Err(e) if e.kind == ErrorKind::Auth => confirmed.push(Confirmed::KeyAccepted),', 'Err(_) => confirmed.push(Confirmed::KeyAccepted),', CL, 'custom', 'fails_for_another_reason'),
    ('open-server-credited', PROBE_C, 'Ok(_) => unconfirmed.push(Unconfirmed::KeyNotEnforced),', 'Ok(_) => {}', CL, 'custom', 'answers_without_the_key'),
    ('capabilities-forgotten', PROBE_C, 'let mut unconfirmed = vec![Unconfirmed::Capabilities];', 'let mut unconfirmed = vec![];', CL, 'custom', 'probe_confirms'),
    ('billing-always-unconfirmed', PROBE_C, 'if connection.billing == Billing::Unknown {', 'if true {', CL, 'custom', 'stated_billing'),
    ('billing-never-unconfirmed', PROBE_C, 'if connection.billing == Billing::Unknown {', 'if false {', CL, 'custom', 'probe_confirms'),
    ('cut-list-not-reported', PROBE_C, 'if listing.truncated {', 'if false {', CL, 'custom', 'cut_list'),
    ('probe-ignores-header-name', PROBE_C, 'config = config.with_key_header(name.clone());', 'let _ = name;', CL, 'custom', 'probe_confirms'),
    ('declared-repeats-allowed', PROBE_C, 'if !seen.insert(choice.id.as_str()) {', 'if !seen.insert(choice.id.as_str()) && false {', CL, 'custom', 'declared_models_are_checked'),
    ('declared-limit-loosened', PROBE_C, 'pub const MAX_DECLARED: usize = 200;', 'pub const MAX_DECLARED: usize = 201;', CL, 'custom', 'declared_models_are_checked'),
    ('declared-limit-unchecked', PROBE_C, 'if choices.len() > MAX_DECLARED {', 'if false {', CL, 'custom', 'declared_models_are_checked'),
    ('test-replaces-declared-models', MANAGE, 'if remember && report.probe.is_none() {', 'if remember {', CL, 'custom', 'never_replaces_declared'),
    ('factory-drops-header', FACTORY, 'config.key_header = connection.key_header.clone();', 'config.key_header = None;', CL, 'custom', 'named_header_from_the_saved'),
]

E = 'craft-ai-comfy-env'
U3 = [
    ('host-suffix-by-text', HOSTS, 'host.len() > rest.len() + 1 && host.ends_with(&format!(".{rest}"))', 'host.ends_with(rest)', E, 'source_basics', 'host_pattern_matches'),
    ('host-exact-by-suffix', HOSTS, 'None => host == pattern,', 'None => host.ends_with(&pattern),', E, 'source_basics', 'host_pattern_matches'),
    ('subdomain-pattern-matches-apex', HOSTS, 'host.len() > rest.len() + 1 && host.ends_with(&format!(".{rest}"))', 'host == rest || host.ends_with(&format!(".{rest}"))', E, 'source_basics', 'host_pattern_matches'),
    ('github-gets-a-token-host', HOSTS, 'download: &["github.com", ".githubusercontent.com"],\n                token_host: None,', 'download: &["github.com", ".githubusercontent.com"],\n                token_host: Some("github.com"),', E, 'source_basics', 'names_its_own_hosts'),
    ('hugging-face-token-host-widened', HOSTS, 'download: &["huggingface.co", ".huggingface.co", ".hf.co"],\n                token_host: Some("huggingface.co"),', 'download: &["huggingface.co", ".huggingface.co", ".hf.co"],\n                token_host: Some(".hf.co"),', E, 'source_basics', 'names_its_own_hosts'),
    ('pickle-runs-no-code', FORMAT, 'self == Self::Pickle', 'false', E, 'source_basics', 'extension_and_pickle'),
    ('bin-not-pickle', FORMAT, '"ckpt" | "pt" | "pth" | "bin" | "pkl" | "pickle"', '"ckpt" | "pt" | "pth" | "pkl" | "pickle"', E, 'source_basics', 'extension_and_pickle'),
    ('zip-not-sniffed', FORMAT, 'if head.starts_with(b"PK\\x03\\x04") ||', 'if false ||', E, 'source_basics', 'bytes_are_read'),
    ('safetensors-header-unbounded', FORMAT, '<= MAX_SAFETENSORS_HEADER', '<= u64::MAX', E, 'source_basics', 'bytes_are_read'),
    ('mislabel-unchecked', FORMAT, 'Self::Safetensors | Self::Gguf if found != claims =>', 'Self::Safetensors | Self::Gguf if false =>', E, 'source_basics', 'what_its_name_says'),
    ('onnx-pickle-allowed', FORMAT, 'Self::Onnx if found != Self::Unknown =>', 'Self::Onnx if false =>', E, 'source_basics', 'what_its_name_says'),
    ('plain-http-accepted', SPEC, '"http" => return Err(SpecError::Insecure),', '"http" => {}', E, 'source_basics', 'could_point_somewhere'),
    ('user-info-accepted', SPEC, 'if !url.username().is_empty() || url.password().is_some() {', 'if false {', E, 'source_basics', 'could_point_somewhere'),
    ('host-matched-by-suffix', SPEC, 'Some("huggingface.co") => hugging_face(&parts),', 'Some(h) if h.ends_with("huggingface.co") => hugging_face(&parts),', E, 'source_basics', 'could_point_somewhere'),
    ('path-not-decoded', SPEC, 'let decoded = percent_decode_str(piece)\n                .decode_utf8()\n                .map_err(|_| SpecError::BadName)?;', 'let decoded = std::borrow::Cow::Borrowed(piece);', E, 'source_basics', 'could_point_somewhere'),
    ('dotfiles-accepted', SPEC, '&& !segment.starts_with(\'.\')', '&& true', E, 'source_basics', 'could_point_somewhere'),
    ('path-characters-unchecked', SPEC, '.any(|c| c.is_control() || "/\\\\:*?\\"<>|".contains(c))', '.any(|_| false)', E, 'source_basics', 'could_point_somewhere'),
    ('local-parent-dir-accepted', SPEC, 'if path.components().any(|c| c == Component::ParentDir) {\n            return Err(SpecError::BadName);\n        }\n        return Ok(SourceSpec::Local', 'if false {\n            return Err(SpecError::BadName);\n        }\n        return Ok(SourceSpec::Local', E, 'source_basics', 'absolute_path'),
    ('revision-unchecked', SPEC, 'if !revision_ok(revision) {', 'if false {', E, 'source_basics', 'could_point_somewhere'),
    ('download-hosts-unchecked', DOWNLOAD, 'if !allowed.iter().any(|pattern| host_matches(pattern, host)) {', 'if false {', E, 'download_sources', 'allowed_hosts'),
    ('size-probe-asks-for-more', DOWNLOAD, 'Some("bytes=0-0".to_owned())', 'Some("bytes=0-1".to_owned())', E, 'download_sources', 'one_byte_range'),
    ('size-probe-accepts-zero', DOWNLOAD, '.filter(|s| *s > 0)', '.filter(|_| true)', E, 'download_sources', 'one_byte_range'),
    ('commit-not-required', HF, '.filter(|s| is_hex(s, 40) && s.bytes().all(|b| !b.is_ascii_uppercase()))', '.filter(|_| true)', E, 'source_resolve', 'asked_file_are_refused'),
    ('lfs-size-mismatch-ignored', HF, '.is_some_and(|s| s != size)', '.is_some_and(|s| s == 0)', E, 'source_resolve', 'host_got_wrong'),
    ('lfs-checksum-unchecked', HF, '.filter(|o| is_hex(o, 64))', '.filter(|_| true)', E, 'source_resolve', 'host_got_wrong'),
    ('gated-ignored', HF, 'Some(Value::String(mode)) => mode != "false",', 'Some(Value::String(_)) => false,', E, 'source_resolve', 'gated_repo_needs'),
    ('gate-alone-needs-no-acceptance', HF, 'accept_required: gated || licence_needs_accepting(&licence_name),', 'accept_required: licence_needs_accepting(&licence_name),', E, 'source_resolve', 'gated_repo_needs'),
    ('directory-accepted-as-file', HF, '&& e.get("type").and_then(Value::as_str) == Some("file")', '&& true', E, 'source_resolve', 'asked_file_are_refused'),
    ('other-path-accepted', HF, 'e.get("path").and_then(Value::as_str) == Some(file)', 'true', E, 'source_resolve', 'asked_file_are_refused'),
    ('token-sent-to-any-host', API, '.is_some_and(|t| host_matches(&t, host))', '.is_some()', E, 'source_resolve', 'token_goes_only'),
    ('listing-host-unchecked', API, 'if !allowed.iter().any(|pattern| host_matches(pattern, host)) {', 'if false {', E, 'source_resolve', 'obey_the_hosts'),
    ('listing-reply-unbounded', API, 'if bytes.len() as u64 > MAX_REPLY_BYTES {', 'if false {', E, 'source_resolve', 'enormous_reply'),
    ('listing-follows-redirects', API, '.max_redirects(0)', '.max_redirects(5)', E, 'source_resolve', 'redirect_from_the_host'),
    ('unfinished-asset-accepted', GH, 'if entry.get("state").and_then(Value::as_str) != Some("uploaded") {', 'if false {', E, 'source_resolve', 'missing_or_unfinished'),
    ('other-digest-algorithm-accepted', GH, '.and_then(|d| d.strip_prefix("sha256:"))', '.and_then(|d| d.split_once(\':\').map(|x| x.1))', E, 'source_resolve', 'without_a_usable_digest'),
    ('civitai-size-crosscheck-removed', CIV, 'if (exact as f64 - approximate).abs() > 2048.0_f64.max(exact as f64 / 100.0) {', 'if false {', E, 'source_resolve', 'pickle_file_is_flagged'),
    ('civitai-download-path-unchecked', CIV, 'u.path().starts_with(DOWNLOAD_PATH)\n                && u.host_str()', 'true\n                && u.host_str()', E, 'source_resolve', 'pickle_file_is_flagged'),
    ('civitai-checksum-unchecked', CIV, 'Some(Value::String(hex)) if is_hex(hex, 64) =>', 'Some(Value::String(hex)) =>', E, 'source_resolve', 'pickle_file_is_flagged'),
    ('civitai-name-unchecked', CIV, '.filter(|n| plain_name(n))', '.filter(|_| true)', E, 'source_resolve', 'pickle_file_is_flagged'),
    ('unsupported-type-resolved', RESOLVE, '&& Format::from_name(name) == Format::Unknown\n        {\n            return Err(unsupported(name));', '&& false\n        {\n            return Err(unsupported(name));', E, 'source_resolve', 'unsupported_file_type'),
    ('directory-taken-as-local-file', RESOLVE, 'if !meta.is_file() {', 'if false {', E, 'source_resolve', 'this_computer_is_described'),
    ('licence-record-ignores-version', REGISTRY, '&& e.pin == candidate.pin\n', '\n', E, 'source_install', 'licence_is_accepted_once'),
    ('licence-record-ignores-link', REGISTRY, '&& e.licence.url == candidate.licence.url\n', '\n', E, 'source_install', 'licence_is_accepted_once'),
    ('licence-record-ignores-kind', REGISTRY, 'e.licence_accepted.is_some()\n                    && e.kind == candidate.kind', 'e.licence_accepted.is_some()\n                    && true', E, 'source_install', 'licence_is_accepted_once'),
    ('local-files-share-a-licence', REGISTRY, '&& (candidate.kind != SourceKind::Local || e.filename == candidate.filename)', '', E, 'source_install', 'copied_and_the_original'),
    ('pickle-consent-ignored', INSTALL, 'if needs.pickle && !consents.pickle {', 'if false {', E, 'source_install', 'pickle_formats_are_refused'),
    ('licence-consent-ignored', INSTALL, 'if needs.licence && !consents.licence && !self.registry.licence_on_record(candidate) {', 'if false {', E, 'source_install', 'licence_is_accepted_once'),
    ('unverified-consent-ignored', INSTALL, 'if needs.unverified && !consents.unverified {', 'if false {', E, 'source_install', 'no_published_checksum'),
    ('gated-without-token-allowed', INSTALL, 'if needs.token && !has_token {', 'if false {', E, 'source_install', 'gated_file_needs'),
    ('existing-file-overwritten', INSTALL, 'if std::fs::symlink_metadata(target_dir.join(&candidate.filename)).is_ok() {', 'if false {', E, 'source_install', 'never_overwritten'),
    ('space-margin-dropped', INSTALL, 'let needed = candidate.size_bytes.saturating_add(MARGIN_BYTES);', 'let needed = candidate.size_bytes;', E, 'source_install', 'disk_space_is_checked'),
    ('space-boundary-off-by-one', INSTALL, 'available < needed', 'available <= needed', E, 'source_install', 'disk_space_is_checked'),
    ('space-reads-the-roomier-folder', INSTALL, '.min();', '.max();', E, 'source_install', 'both_the_staging'),
    ('size-limit-loosened', INSTALL, 'candidate.size_bytes > MAX_FILE_BYTES', 'candidate.size_bytes > MAX_FILE_BYTES + 1', E, 'source_install', 'absurd_size'),
    ('empty-file-accepted', INSTALL, 'candidate.size_bytes == 0 ||', '', E, 'source_install', 'absurd_size'),
    ('format-bytes-unread', INSTALL, 'Format::check(name, &head).map_err(|problem| {', 'Ok::<Format, FormatProblem>(Format::from_name(name)).map_err(|problem| {', E, 'source_install', 'pickle_bytes_under_a_safe_name'),
    ('disguised-file-kept-in-staging', INSTALL, 'let _ = std::fs::remove_file(&staged_file);\n                return Err(refusal);', 'return Err(refusal);', E, 'source_install', 'pickle_bytes_under_a_safe_name'),
    ('observed-called-published', INSTALL, 'Ok((observed, Checksum::Observed))', 'Ok((observed, Checksum::Published))', E, 'source_install', 'no_published_checksum'),
    ('staging-shared-between-sources', INSTALL, 'self.staging.join(name)', 'self.staging.clone()', E, 'source_install', 'do_not_share_a_partial'),
    ('staged-leftover-trusted', INSTALL, '        let _ = std::fs::remove_file(&staged_file);\n\n        let (digest', '        let (digest', E, 'source_install', 'leftover_file_in_staging'),
    ('local-copy-size-unchecked', INSTALL, 'if size != candidate.size_bytes {', 'if false {', E, 'source_install', 'copied_and_the_original'),
    ('folder-unchecked', INSTALL, 'let target_dir = self.comfy.models_dir_for(folder).ok_or(Refusal::Folder)?;', 'let target_dir = self.comfy.base.join("models").join(folder);', E, 'source_install', 'cannot_leave_the_models'),
    ('settings-hosts-not-passed-on', C + 'ai-comfy-env/src/source/settings.rs', 'allowed_hosts: Some(self.download_hosts(kind)),', 'allowed_hosts: None,', E, 'source_install', 'reach_the_downloader'),
    ('registry-overwrites-damage', REGISTRY, 'serde_json::from_str(&text).map_err(|_| unreadable())', 'Ok(serde_json::from_str(&text).unwrap_or_default())', E, 'source_install', 'read_with_care'),
]
# `..` is also stopped by the trailing-dot rule, so this mutant lifts both checks for `..` alone.
DOT_DOT_OLD = """if part.is_empty() || part == "." || part == ".." || part.len() > 255 {
        return false;
    }
    if part.ends_with('.') || part.ends_with(' ') {"""
DOT_DOT_NEW = """if part.is_empty() || part.len() > 255 {
        return false;
    }
    if (part.ends_with('.') && part != "..") || part.ends_with(' ') {"""
U4 = [
    ('pack-plain-http-accepted', PACK, '"http" => return Err(SpecError::Insecure),', '"http" => {}', E, 'nodepack', 'not_a_plain_https'),
    ('pack-user-info-accepted', PACK, 'if !url.username().is_empty() || url.password().is_some() {', 'if false {', E, 'nodepack', 'not_a_plain_https'),
    ('pack-any-host-accepted', PACK, 'if url.host_str() != Some("github.com") || url.port().is_some() {', 'if url.port().is_some() {', E, 'nodepack', 'not_a_plain_https'),
    ('pack-port-accepted', PACK, 'if url.host_str() != Some("github.com") || url.port().is_some() {', 'if url.host_str() != Some("github.com") {', E, 'nodepack', 'not_a_plain_https'),
    ('pack-ref-unchecked', PACK, 'if reference.is_some_and(|r| !revision_ok(r)) {', 'if false {', E, 'nodepack', 'not_a_plain_https'),
    ('pack-names-unchecked', PACK, 'if !simple_name(owner) || !simple_name(repo) {', 'if false {', E, 'nodepack', 'not_a_plain_https'),
    ('pack-private-repo-allowed', PACK_RESOLVE, 'if info.get("private").and_then(Value::as_bool) != Some(false) {', 'if false {', E, 'nodepack', 'private_repository'),
    ('pack-commit-unchecked', PACK_RESOLVE, '.filter(|s| is_hex(s, 40) && s.bytes().all(|b| !b.is_ascii_uppercase()))', '.filter(|_| true)', E, 'nodepack', 'private_repository'),
    ('pack-subject-unclean', PACK_RESOLVE, 'let clean: String = line.chars().filter(|c| !c.is_control()).take(120).collect();', 'let clean: String = line.chars().take(120).collect();', E, 'nodepack', 'one_short_clean_line'),
    ('pack-subject-unbounded', PACK_RESOLVE, '.take(120)', '.take(1200)', E, 'nodepack', 'one_short_clean_line'),
    ('pack-archive-unpinned', PACK_RESOLVE, '&format!("{sha}.tar.gz")', '&format!("{}.tar.gz", spec.reference.as_deref().unwrap_or("main"))', E, 'nodepack', 'pinned_to_a_commit'),
    ('pack-consent-for-any-commit', PACK_INSTALL, 'if !consent.is_some_and(|c| c.commit == candidate.commit) {', 'if consent.is_none() {', E, 'nodepack', 'consent_to_run_code'),
    ('pack-consent-not-needed', PACK_INSTALL, 'if !consent.is_some_and(|c| c.commit == candidate.commit) {', 'if false {', E, 'nodepack', 'consent_to_run_code'),
    ('pack-folder-checked-after-download', PACK_INSTALL, 'if std::fs::symlink_metadata(&target).is_ok() {\n            return Err(PackError::Refused(PackRefusal::FolderTaken));\n        }\n        if cancel', 'if false {\n            return Err(PackError::Refused(PackRefusal::FolderTaken));\n        }\n        if cancel', E, 'nodepack', 'existing_folder_is_never'),
    ('pack-staging-kept-on-refusal', PACK_INSTALL, 'let _ = std::fs::remove_dir_all(&stage);\n                return Err(e);', 'return Err(e);', E, 'nodepack_archive', 'hostile_archives'),
    ('pack-requirements-not-flagged', PACK_INSTALL, '.any(|n| n == "requirements.txt")', '.any(|n| n == "requirements.tx")', E, 'nodepack', 'unpacked_exactly_as_shipped'),
    ('pack-install-script-not-flagged', PACK_INSTALL, '.any(|n| n == "install.py" || n == "setup.py")', '.any(|n| n == "setup.py")', E, 'nodepack', 'unpacked_exactly_as_shipped'),
    ('pack-removes-what-it-did-not-install', PACK_INSTALL, 'if !self.registry.packs()?.iter().any(|p| p.folder == folder) {', 'if false {', E, 'nodepack', 'recorded_pack_can_be_removed'),
    ('pack-follows-links-on-removal', PACK_INSTALL, 'Ok(meta) if meta.file_type().is_symlink() => {', 'Ok(meta) if false => {', E, 'nodepack', 'folder_that_is_a_link'),
    ('archive-links-and-devices-accepted', PACK_ARCHIVE, "b'0' | 0 | b'5' => {}", "b'0' | 0 | b'5' | b'1' | b'2' | b'3' => {}", E, 'nodepack_archive', 'hostile_archives'),
    ('archive-dot-dot-accepted', PACK_ARCHIVE, DOT_DOT_OLD, DOT_DOT_NEW, E, 'nodepack_archive', 'hostile_archives'),
    ('archive-backslash-accepted', PACK_ARCHIVE, r'"\\:<>\"|?*".contains(c)', r'":<>\"|?*".contains(c)', E, 'nodepack_archive', 'hostile_archives'),
    ('archive-drive-letter-accepted', PACK_ARCHIVE, r'"\\:<>\"|?*".contains(c)', r'"\\<>\"|?*".contains(c)', E, 'nodepack_archive', 'hostile_archives'),
    ('archive-device-names-accepted', PACK_ARCHIVE, '!RESERVED.contains(&stem.as_str())', 'true', E, 'nodepack_archive', 'hostile_archives'),
    ('archive-trailing-dot-accepted', PACK_ARCHIVE, "if part.ends_with('.') || part.ends_with(' ') {", 'if false {', E, 'nodepack_archive', 'hostile_archives'),
    ('archive-case-duplicates-accepted', PACK_ARCHIVE, 'if !seen.insert(rest.join("/").to_lowercase()) {', 'if !seen.insert(rest.join("/")) {', E, 'nodepack_archive', 'hostile_archives'),
    ('archive-second-top-folder-accepted', PACK_ARCHIVE, 'Some(_) => return Err(unsafe_archive("the archive has more than one top folder")),', 'Some(_) => {}', E, 'nodepack_archive', 'hostile_archives'),
    ('archive-checksum-unchecked', PACK_ARCHIVE, 'if sum == stored {', 'if true {', E, 'nodepack_archive', 'hostile_archives'),
    ('archive-entry-limit-unchecked', PACK_ARCHIVE, 'if entries > limits.max_entries {', 'if false {', E, 'nodepack_archive', 'size_limits'),
    ('archive-size-limits-unchecked', PACK_ARCHIVE, 'if size > limits.max_file || bytes.saturating_add(size) > limits.max_expanded {', 'if false {', E, 'nodepack_archive', 'size_limits'),
    ('archive-pax-size-accepted', PACK_ARCHIVE, '"size" | "linkpath" => {', '"linkpath" => {', E, 'nodepack_archive', 'hostile_archives'),
    ('archive-empty-accepted', PACK_ARCHIVE, 'if files == 0 {', 'if false {', E, 'nodepack_archive', 'hostile_archives'),
    ('download-body-unbounded', DOWNLOAD, 'if bytes.len() as u64 > max_bytes {', 'if false {', E, 'download_sources', 'only_up_to_its_limit'),
]

GROUPS = {'u1': U1, 'u3': U3, 'u4': U4}
# Mutants whose effect is also refused by a second, independent check on this machine. They are
# recorded as equivalent here, with the reason, and are never counted as caught.
EQUIVALENT = {
    'directory-taken-as-local-file': 'on Windows a directory reports length 0, which the size check also refuses; elsewhere the is_file check is what refuses it',
}
TIMEOUTS = {}


def main():
    group = sys.argv[1] if len(sys.argv) > 1 else 'u1'
    only = sys.argv[2] if len(sys.argv) > 2 else ''
    if group not in GROUPS:
        raise SystemExit('group must be one of ' + ', '.join(GROUPS))
    results = []
    if only == '--dry':
        # Check every mutation still has exactly one target, without running cargo.
        bad = []
        for name, rel, old, *_ in GROUPS[group]:
            text = (ROOT / rel).read_bytes().decode('utf-8').replace(CRLF, LF)
            if text.count(old) != 1:
                bad.append((name, text.count(old)))
        print('mutations:', len(GROUPS[group]), 'without exactly one target:', bad)
        raise SystemExit(1 if bad else 0)
    for name, rel, old, new, package, target, test in GROUPS[group]:
        if only not in name:
            continue
        path = ROOT / rel
        original = path.read_bytes()
        text = original.decode('utf-8').replace('\r\n', '\n')
        assert text.count(old) == 1, (name, 'mutation did not have exactly one target', text.count(old))
        newline = '\r\n' if b'\r\n' in original else '\n'
        try:
            path.write_bytes(text.replace(old, new).replace('\n', newline).encode('utf-8'))
            command = ['cargo', 'test', '-p', package, '--test', target, test, '-j', '8']
            proc = subprocess.Popen(command, cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
            timed_out = False
            try:
                log, _ = proc.communicate(timeout=TIMEOUTS.get(name, 400))
            except subprocess.TimeoutExpired:
                timed_out = True
                subprocess.run(['taskkill', '/T', '/F', '/PID', str(proc.pid)], capture_output=True)
                log, _ = proc.communicate()
                log += '\nTIMED OUT: the mutated code made the test hang'
            (OUT / ('mutation-' + name + '.txt')).write_text(scrub(log), encoding='utf-8')
            caught = timed_out or (proc.returncode != 0 and 'test result: FAILED' in log)
            equivalent = (not caught) and name in EQUIVALENT
            results.append({'mutation': name, 'caught_by_runtime_test': caught, 'timed_out': timed_out, 'equivalent': EQUIVALENT[name] if equivalent else None})
            print(name, 'CAUGHT' if caught else ('EQUIVALENT' if equivalent else 'NOT CAUGHT'), flush=True)
        finally:
            path.write_bytes(original)
            os.utime(path, None)
            # The drive-letter mutant really does write `C:evil.py` into the working folder of the
            # test (a drive-relative path), which is what the check exists to stop. Tidy it away.
            (ROOT / 'crates/ai-comfy-env/evil.py').unlink(missing_ok=True)
    saved = OUT / (group + '-mutations.json')
    if only and saved.exists():
        # A filtered rerun updates its own entries and keeps the rest.
        merged = {r['mutation']: r for r in json.loads(saved.read_text(encoding='utf-8'))}
        merged.update({r['mutation']: r for r in results})
        results = list(merged.values())
    saved.write_text(json.dumps(results, indent=2), encoding='utf-8')
    survivors = [r['mutation'] for r in results if not r['caught_by_runtime_test'] and not r.get('equivalent')]
    equivalent = [r['mutation'] for r in results if r.get('equivalent')]
    print(json.dumps({'mutations': len(results), 'caught': sum(r['caught_by_runtime_test'] for r in results), 'equivalent': equivalent, 'survivors': survivors}))
    if survivors:
        raise SystemExit(1)


main()
