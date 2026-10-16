//! Spike 3 — updater without Tauri.
//!
//! Replicates the tauri-plugin-updater flow in plain Rust:
//!   1. GET `latest.json` (worker endpoint, then GitHub fallback)
//!   2. Pick the windows-x86_64 platform entry
//!   3. Download the NSIS installer to %TEMP%
//!   4. Verify the minisign signature against the bundled pubkey
//!   5. Report "ready to launch" — does NOT actually run the installer.
//!
//! If this passes, the only Tauri-provided piece we'd lose is the
//! fetch+verify+launch glue, which is ~150 lines of std Rust.
//!
//! Run: cargo run -p capto-gpui-spike --bin updater-spike

use std::io::Read as _;

use minisign_verify::{PublicKey, Signature};

/// Same value as `plugins.updater.pubkey` in tauri.conf.json — base64 of the
/// minisign public-key file text (untrusted comment + base64 key line).
const PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDhCQTY5N0YxMDhBMjY0NUEKUldSYVpLSUk4WmVtaTJ6UE5ic0t5OWJKMmxvTjltY2tDTjhpbWx3SlI0MFJlbHFxeERjMndCcFEK";

const ENDPOINTS: &[&str] = &[
    "https://capto-update-proxy.elwina-vardal.workers.dev/updates/latest.json",
    "https://github.com/ciryanna/Capto/releases/download/updater/latest.json",
];

const CURRENT_VERSION: &str = "1.0.0";
const PLATFORM: &str = "windows-x86_64";

fn main() -> anyhow::Result<()> {
    let client = reqwest::blocking::Client::builder()
        .user_agent(format!("capto-updater-spike/{CURRENT_VERSION}"))
        .timeout(std::time::Duration::from_secs(120))
        .build()?;

    // ── 1. fetch latest.json ────────────────────────────────────────────
    let mut body = String::new();
    let mut source = None;
    for ep in ENDPOINTS {
        match client.get(*ep).send() {
            Ok(resp) if resp.status().is_success() => {
                println!("[1] latest.json <- {ep}");
                body = resp.text()?;
                source = Some(*ep);
                break;
            }
            Ok(resp) => println!("    {ep} -> HTTP {}, trying next", resp.status()),
            Err(e) => println!("    {ep} -> {e}, trying next"),
        }
    }
    if source.is_none() {
        anyhow::bail!("all endpoints failed");
    }
    let json: serde_json::Value = serde_json::from_str(&body)?;
    let version = json["version"].as_str().unwrap_or("?").to_string();
    println!("    remote version: {version} (current {CURRENT_VERSION})");

    // ── 2. platform entry ───────────────────────────────────────────────
    let entry = &json["platforms"][PLATFORM];
    let url = entry["url"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("no url for {PLATFORM}"))?
        .to_string();
    let sig_b64 = entry["signature"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("no signature"))?
        .to_string();
    println!("[2] platform {PLATFORM}\n    url: {url}");

    if version == CURRENT_VERSION {
        println!("    (version == current; continuing anyway to prove verify works)");
    }

    // ── 3. download installer ───────────────────────────────────────────
    let tmp = std::env::temp_dir().join(format!("capto-update-{version}.exe"));
    print!("[3] downloading -> {} ", tmp.display());
    let t0 = std::time::Instant::now();
    let mut resp = client.get(&url).send()?.error_for_status()?;
    let mut file = std::fs::File::create(&tmp)?;
    let mut buf = [0u8; 1 << 16];
    let mut total = 0u64;
    loop {
        let n = resp.read(&mut buf)?;
        if n == 0 {
            break;
        }
        std::io::Write::write_all(&mut file, &buf[..n])?;
        total += n as u64;
    }
    let dt = t0.elapsed();
    println!(
        "done ({:.1} MB in {:.1?}, {:.1} MB/s)",
        total as f64 / 1e6,
        dt,
        total as f64 / 1e6 / dt.as_secs_f64().max(0.001)
    );

    // ── 4. minisign verify ──────────────────────────────────────────────
    // tauri.conf pubkey = base64(minisign.pub file text); pull the key line out.
    let pk_text = String::from_utf8(base64_decode(&sig_b64)?)
        .or_else(|_| Ok::<String, anyhow::Error>(sig_b64.clone()))?;
    let sig = Signature::decode(&pk_text)
        .or_else(|_| {
            // alternative packing: sig field is the raw .sig line itself
            Signature::decode(&format!(
                "untrusted comment: signature from minisign secret key\n{sig_b64}"
            ))
        })
        .map_err(|e| anyhow::anyhow!("signature decode: {e:?}"))?;

    let pk_file_text = String::from_utf8(base64_decode(PUBKEY)?)?;
    let pk_line = pk_file_text
        .lines()
        .find(|l| l.starts_with("RW"))
        .ok_or_else(|| anyhow::anyhow!("pubkey line not found"))?;
    let pk = PublicKey::from_base64(pk_line.trim())
        .map_err(|e| anyhow::anyhow!("pubkey decode: {e:?}"))?;

    let bytes = std::fs::read(&tmp)?;
    pk.verify(&bytes, &sig, false)
        .map_err(|e| anyhow::anyhow!("VERIFY FAILED: {e:?}"))?;
    println!("[4] minisign signature: VALID");

    // ── 5. ready ────────────────────────────────────────────────────────
    // A real updater would now run: <file> with the NSIS passive flag and
    // exit the app so the installer can overwrite it. We stop here.
    println!(
        "[5] READY — would launch `{}` (NSIS passive) and quit",
        tmp.display()
    );
    println!("    cleanup: {}", tmp.display());
    Ok(())
}

fn base64_decode(s: &str) -> anyhow::Result<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut acc = 0u32;
    let mut nbits = 0u32;
    for &b in s.as_bytes() {
        let v = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            b' ' | b'\n' | b'\r' | b'\t' => continue,
            _ => anyhow::bail!("bad base64 char {b}"),
        } as u32;
        acc = (acc << 6) | v;
        nbits += 6;
        if nbits >= 8 {
            nbits -= 8;
            out.push((acc >> nbits) as u8);
        }
    }
    Ok(out)
}
