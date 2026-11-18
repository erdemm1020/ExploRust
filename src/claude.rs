//! Asking Claude about a file or a folder. Raw HTTP through `curl` (no SDK for Rust):
//! the API key lives in the macOS Keychain and never goes on a command line, and only
//! what the user chose to ask about is sent.

use std::{
    fs,
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
};

use serde_json::{Value, json};

use crate::config::ClaudeModel;

const KEYCHAIN_SERVICE: &str = "ExploRust Claude API key";
const KEYCHAIN_ACCOUNT: &str = "explorust";
/// The Messages API version of today (see the docs: it is a date).
const API_VERSION: &str = "2026-10-21";

impl ClaudeModel {
    pub fn id(self) -> &'static str {
        match self {
            Self::Opus => "claude-opus-5-5",
            Self::Sonnet => "claude-sonnet-5-5",
            Self::Haiku => "claude-haiku-5-5",
        }
    }
}

// --- The key -----------------------------------------------------------------------

/// The saved API key, if any.
pub fn api_key() -> Option<String> {
    keychain_get(KEYCHAIN_SERVICE)
}

pub fn save_key(key: &str) -> Result<(), String> {
    keychain_set(KEYCHAIN_SERVICE, key)
}

pub fn remove_key() -> Result<(), String> {
    keychain_delete(KEYCHAIN_SERVICE)
}

/// Does `text` look like an Anthropic API key? (Checked before it is saved.)
pub fn looks_like_key(text: &str) -> bool {
    let t = text.trim();
    t.starts_with("sk-ant-") && t.len() > 20 && t.chars().all(|c| c.is_ascii_graphic())
}

fn keychain_get(service: &str) -> Option<String> {
    let out = Command::new("/usr/bin/security")
        .args([
            "find-generic-password",
            "-a",
            KEYCHAIN_ACCOUNT,
            "-s",
            service,
            "-w",
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let key = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (out.status.success() && !key.is_empty()).then_some(key)
}

fn keychain_set(service: &str, key: &str) -> Result<(), String> {
    let out = Command::new("/usr/bin/security")
        .args([
            "add-generic-password",
            "-a",
            KEYCHAIN_ACCOUNT,
            "-s",
            service,
            "-U",
            "-w",
            key,
        ])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err("the Keychain refused the key".into())
    }
}

fn keychain_delete(service: &str) -> Result<(), String> {
    let out = Command::new("/usr/bin/security")
        .args([
            "delete-generic-password",
            "-a",
            KEYCHAIN_ACCOUNT,
            "-s",
            service,
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err("there was no saved key".into())
    }
}

// --- The request -------------------------------------------------------------------

fn endpoint() -> String {
    #[cfg(debug_assertions)]
    if let Ok(url) = std::env::var("EXPLORUST_CLAUDE_URL") {
        return url;
    }
    "https://api.anthropic.com/v1/messages".into()
}

/// Ask `model`. Returns the text of the answer.
pub fn ask(key: &str, model: ClaudeModel, system: &str, user: &str) -> Result<String, String> {
    ask_at(&endpoint(), key, model, system, user)
}

fn ask_at(
    url: &str,
    key: &str,
    model: ClaudeModel,
    system: &str,
    user: &str,
) -> Result<String, String> {
    let body = request_body(model, system, user);
    let mut headers = vec![
        format!("x-api-key: {key}"),
        format!("anthropic-version: {API_VERSION}"),
        "content-type: application/json".to_string(),
    ];
    // A policy refusal is retried by the API on another model, inside the same call.
    if model != ClaudeModel::Haiku {
        headers.push("anthropic-beta: server-side-fallback-2026-07-01".into());
    }
    let response = post(url, &headers, &body)?;
    answer_text(&response)
}

/// The JSON of a request: one short question, low effort (these are quick answers).
fn request_body(model: ClaudeModel, system: &str, user: &str) -> String {
    let mut body = json!({
        "model": model.id(),
        "max_tokens": 1500,
        "system": system,
        "messages": [{ "role": "user", "content": user }],
        "output_config": { "effort": "low" },
    });
    if model != ClaudeModel::Haiku {
        body["fallbacks"] = json!("default");
    }
    body.to_string()
}

/// The text of an answer, or the reason there is none.
fn answer_text(response: &str) -> Result<String, String> {
    let v: Value =
        serde_json::from_str(response).map_err(|_| "unexpected answer from Claude".to_string())?;
    if let Some(msg) = v["error"]["message"].as_str() {
        return Err(msg.to_string());
    }
    if v["stop_reason"] == "refusal" {
        return Err("Claude declined to answer this".into());
    }
    let text: String = v["content"]
        .as_array()
        .map(|blocks| {
            blocks
                .iter()
                .filter(|b| b["type"] == "text")
                .filter_map(|b| b["text"].as_str())
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default();
    let text = text.trim().to_string();
    if text.is_empty() {
        Err("Claude sent no text".into())
    } else {
        Ok(text)
    }
}

/// POST `body`. Headers (the key among them) and the URL go to curl through its standard
/// input, so no other process can read them in the process list.
fn post(url: &str, headers: &[String], body: &str) -> Result<String, String> {
    let quote = |s: &str| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""));
    let mut config = format!("url = {}\nrequest = \"POST\"\nmax-time = 120\n", quote(url));
    for h in headers {
        config.push_str(&format!("header = {}\n", quote(h)));
    }
    config.push_str(&format!("data = {}\n", quote(body)));

    let mut child = Command::new("/usr/bin/curl")
        .args(["-sS", "-K", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    child
        .stdin
        .take()
        .ok_or("no stdin")?
        .write_all(config.as_bytes())
        .map_err(|e| e.to_string())?;
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        // Not curl's own words: they could repeat the address.
        Err("could not reach Claude (check the connection)".into())
    }
}

// --- What is sent ------------------------------------------------------------------

/// Names that usually hold secrets: never sent, even when asked.
pub fn looks_secret(path: &Path) -> bool {
    let name = path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().to_lowercase());
    let secret_names = [
        "id_rsa",
        "id_ed25519",
        "id_ecdsa",
        "id_dsa",
        ".netrc",
        ".npmrc",
        ".pgpass",
        "credentials",
        "secrets",
        ".git-credentials",
    ];
    let secret_ext = [
        ".pem",
        ".key",
        ".p12",
        ".pfx",
        ".keychain",
        ".keychain-db",
        ".kdbx",
        ".env",
    ];
    name.starts_with(".env")
        || secret_names.contains(&name.as_str())
        || secret_ext.iter().any(|e| name.ends_with(e))
        || path.components().any(|c| {
            matches!(
                c.as_os_str().to_str(),
                Some(".ssh" | ".aws" | ".gnupg" | "Keychains")
            )
        })
}

/// The first `max` bytes of a text file. `None` for a binary file.
pub fn read_head(path: &Path, max: usize) -> Option<String> {
    let mut buf = vec![0u8; max];
    let n = fs::File::open(path).ok()?.read(&mut buf).ok()?;
    buf.truncate(n);
    if buf.contains(&0) {
        return None;
    }
    Some(String::from_utf8_lossy(&buf).into_owned())
}

pub const SYSTEM: &str = "You are a helper built into a file manager. Answer in plain text without \
markdown, briefly, in the language of the file names or content when it is clear, else in English.";

/// "Explain this file".
pub fn explain_file_prompt(name: &str, size: u64, head: Option<&str>) -> String {
    match head {
        Some(text) => format!(
            "Explain what this file is and what it does, in 4 to 8 lines.\n\
             File: {name} ({size} bytes). Start of its content:\n\n{text}"
        ),
        None => format!(
            "This is a binary file named \"{name}\" ({size} bytes). From its name and size only, \
             say what it most likely is, in 2 or 3 lines, and say that you could not look inside."
        ),
    }
}

/// "Explain this folder": a listing and, if there is one, the start of its README.
pub fn explain_folder_prompt(
    name: &str,
    listing: &[(String, bool, u64)],
    readme: Option<&str>,
) -> String {
    let mut text = format!(
        "Explain what this folder is for, in 4 to 8 lines, from its content.\nFolder: {name}\n\n"
    );
    text.push_str(&listing_text(listing));
    if let Some(readme) = readme {
        text.push_str(&format!("\nStart of its README:\n{readme}\n"));
    }
    text
}

/// "How would you organize this folder?".
pub fn organize_prompt(name: &str, listing: &[(String, bool, u64)]) -> String {
    format!(
        "Suggest, in at most 10 short lines, how to organize the files of the folder \"{name}\" \
         into subfolders, naming the subfolders and which files go where. Do not invent files.\n\n{}",
        listing_text(listing)
    )
}

fn listing_text(listing: &[(String, bool, u64)]) -> String {
    let mut out = String::new();
    for (name, is_dir, size) in listing.iter().take(200) {
        if *is_dir {
            out.push_str(&format!("{name}/\n"));
        } else {
            out.push_str(&format!("{name} ({size} bytes)\n"));
        }
    }
    if listing.len() > 200 {
        out.push_str(&format!("... and {} more\n", listing.len() - 200));
    }
    out
}

/// "Suggest a file name".
pub fn name_prompt(current: &str, head: Option<&str>) -> String {
    let content = head.map_or_else(
        || "(binary file: no content to show)".to_string(),
        |t| format!("Start of its content:\n\n{t}"),
    );
    format!(
        "Suggest one clear, short file name for this file (keep its extension). \
         Reply with the name only.\nCurrent name: {current}\n{content}"
    )
}

/// The suggested name from Claude's answer: one line, no quotes, nothing that is not a file name.
pub fn clean_name(answer: &str, current: &str) -> Option<String> {
    let line = answer.lines().map(str::trim).find(|l| !l.is_empty())?;
    let name: String = line
        .trim_matches(|c| "\"'`*_ ".contains(c))
        .chars()
        .filter(|c| !matches!(c, '/' | '\\' | ':' | '\0') && !c.is_control())
        .take(120)
        .collect();
    let name = name.trim().trim_start_matches('.').to_string();
    if name.is_empty() {
        return None;
    }
    // Keep the extension when Claude dropped it.
    let ext = Path::new(current).extension().and_then(|e| e.to_str());
    Some(match (ext, Path::new(&name).extension()) {
        (Some(ext), None) => format!("{name}.{ext}"),
        _ => name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn requests_use_the_current_models_and_low_effort() {
        for (model, id, fallback) in [
            (ClaudeModel::Opus, "claude-opus-5-5", true),
            (ClaudeModel::Sonnet, "claude-sonnet-5-5", true),
            (ClaudeModel::Haiku, "claude-haiku-5-5", false),
        ] {
            let v: Value =
                serde_json::from_str(&request_body(model, "sys", "hello \"quoted\"")).unwrap();
            assert_eq!(v["model"], id);
            assert_eq!(v["system"], "sys");
            assert_eq!(v["messages"][0]["role"], "user");
            assert_eq!(v["messages"][0]["content"], "hello \"quoted\"");
            assert_eq!(v["output_config"]["effort"], "low");
            assert_eq!(v.get("fallbacks").is_some(), fallback, "{id}");
            // No removed parameters (they are an error on these models).
            for gone in ["temperature", "top_p", "top_k", "thinking"] {
                assert!(v.get(gone).is_none(), "{gone}");
            }
        }
    }

    #[test]
    fn reads_answers_errors_and_refusals() {
        let ok = r#"{"content":[{"type":"thinking","thinking":""},{"type":"text","text":"Hello "},{"type":"text","text":"world"}],"stop_reason":"end_turn"}"#;
        assert_eq!(answer_text(ok).unwrap(), "Hello world");
        let err = r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#;
        assert_eq!(answer_text(err).unwrap_err(), "invalid x-api-key");
        let refused = r#"{"content":[],"stop_reason":"refusal","stop_details":{"type":"refusal","category":"cyber"}}"#;
        assert!(answer_text(refused).unwrap_err().contains("declined"));
        assert!(answer_text("<html>").is_err());
        assert!(answer_text(r#"{"content":[{"type":"text","text":"  "}]}"#).is_err());
    }

    /// A one-shot local server: returns what it received, and answers `reply`.
    fn serve_once(reply: &'static str) -> (String, std::sync::mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!(
            "http://127.0.0.1:{}/v1/messages",
            listener.local_addr().unwrap().port()
        );
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut data = Vec::new();
            let mut buf = [0u8; 8192];
            loop {
                let n = s.read(&mut buf).unwrap_or(0);
                data.extend_from_slice(&buf[..n]);
                let text = String::from_utf8_lossy(&data).into_owned();
                if let Some(i) = text.find("\r\n\r\n") {
                    let len = text
                        .lines()
                        .find_map(|l| {
                            l.to_lowercase()
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse::<usize>().unwrap())
                        })
                        .unwrap_or(0);
                    if data.len() >= i + 4 + len {
                        break;
                    }
                }
                if n == 0 {
                    break;
                }
            }
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                reply.len()
            );
            let _ = s.write_all(head.as_bytes());
            let _ = s.write_all(reply.as_bytes());
            tx.send(String::from_utf8_lossy(&data).into_owned())
                .unwrap();
        });
        (url, rx)
    }

    #[test]
    fn sends_the_key_in_a_header_and_reads_the_answer() {
        let (url, rx) = serve_once(
            r#"{"content":[{"type":"text","text":"It is a test."}],"stop_reason":"end_turn"}"#,
        );
        let answer = ask_at(
            &url,
            "sk-ant-test-key-1234567890",
            ClaudeModel::Opus,
            "sys",
            "what is this?",
        )
        .unwrap();
        assert_eq!(answer, "It is a test.");
        let request = rx.recv().unwrap().to_lowercase();
        assert!(request.contains("x-api-key: sk-ant-test-key-1234567890"));
        assert!(request.contains(&format!("anthropic-version: {API_VERSION}")));
        assert!(request.contains("anthropic-beta: server-side-fallback-2026-07-01"));
        assert!(request.contains("\"model\":\"claude-opus-5-5\""));
    }

    #[test]
    fn a_network_failure_does_not_leak_the_url_or_the_key() {
        let err = ask_at(
            "http://127.0.0.1:1/secret-path",
            "sk-ant-secret-key-1234567890",
            ClaudeModel::Haiku,
            "s",
            "u",
        )
        .unwrap_err();
        assert!(!err.contains("secret"), "{err}");
    }

    #[test]
    fn keys_are_recognized_before_saving() {
        assert!(looks_like_key("sk-ant-api03-abcdefghijklmnopqrstuvwxyz"));
        assert!(looks_like_key(
            "  sk-ant-api03-abcdefghijklmnopqrstuvwxyz\n"
        ));
        assert!(!looks_like_key("sk-proj-abcdefghijklmnopqrstuvwxyz"));
        assert!(!looks_like_key("hello world"));
        assert!(!looks_like_key("sk-ant-short"));
        assert!(!looks_like_key("sk-ant-has space inside it, not a key"));
    }

    #[test]
    fn the_key_round_trips_through_the_keychain() {
        // A throwaway item: never the user's real one.
        let service = format!("ExploRust test key {}", std::process::id());
        assert_eq!(keychain_get(&service), None);
        keychain_set(&service, "sk-ant-test-0123456789abcdef").unwrap();
        assert_eq!(
            keychain_get(&service).as_deref(),
            Some("sk-ant-test-0123456789abcdef")
        );
        keychain_set(&service, "sk-ant-test-replaced-0123456789").unwrap();
        assert_eq!(
            keychain_get(&service).as_deref(),
            Some("sk-ant-test-replaced-0123456789")
        );
        keychain_delete(&service).unwrap();
        assert_eq!(keychain_get(&service), None);
    }

    #[test]
    fn secret_looking_files_are_never_sent() {
        for p in [
            ".env",
            ".env.local",
            "/home/me/.ssh/config",
            "id_rsa",
            "server.pem",
            "my.key",
            "/a/.aws/credentials",
            "vault.kdbx",
            ".netrc",
        ] {
            assert!(looks_secret(Path::new(p)), "{p}");
        }
        for p in [
            "notes.txt",
            "main.rs",
            "environment.md",
            "keynote.pdf",
            "/home/me/code/lib.rs",
        ] {
            assert!(!looks_secret(Path::new(p)), "{p}");
        }
    }

    #[test]
    fn reads_the_start_of_text_files_only() {
        let dir = std::env::temp_dir().join(format!("explorust-claude-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("long.txt"), "x".repeat(50_000)).unwrap();
        fs::write(dir.join("bin"), [1u8, 2, 0, 3]).unwrap();
        assert_eq!(read_head(&dir.join("long.txt"), 1000).unwrap().len(), 1000);
        assert_eq!(read_head(&dir.join("bin"), 1000), None);
        assert_eq!(read_head(&dir.join("missing"), 10), None);
    }

    #[test]
    fn prompts_carry_what_they_should() {
        let p = explain_file_prompt("a.rs", 12, Some("fn main() {}"));
        assert!(p.contains("a.rs") && p.contains("fn main() {}"));
        assert!(explain_file_prompt("a.bin", 12, None).contains("binary"));
        let listing: Vec<_> = (0..250)
            .map(|i| (format!("f{i}"), false, i as u64))
            .collect();
        let folder = explain_folder_prompt("proj", &listing, Some("# Title"));
        assert!(folder.contains("f199") && !folder.contains("f200 ") && folder.contains("50 more"));
        assert!(folder.contains("# Title"));
        assert!(organize_prompt("docs", &listing[..3]).contains("f2 (2 bytes)"));
        assert!(name_prompt("IMG_1.jpg", None).contains("binary"));
    }

    #[test]
    fn suggested_names_are_made_safe() {
        assert_eq!(
            clean_name("\"Quarterly report.pdf\"\nBecause...", "scan1.pdf").unwrap(),
            "Quarterly report.pdf"
        );
        assert_eq!(clean_name("`budget`", "x.xlsx").unwrap(), "budget.xlsx");
        // Nothing that climbs out of the folder or hides the file.
        assert_eq!(
            clean_name("../../etc/passwd", "a.txt").unwrap(),
            "....etcpasswd.txt".trim_start_matches('.').to_string()
        );
        assert_eq!(clean_name("...hidden", "a.txt").unwrap(), "hidden.txt");
        assert_eq!(clean_name("   \n  ", "a.txt"), None);
        assert_eq!(clean_name("a/b:c", "n").unwrap(), "abc");
    }
}
