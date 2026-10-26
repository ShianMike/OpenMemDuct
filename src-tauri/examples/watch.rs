use std::time::{Duration, Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.is_empty() || args.len() > 2 {
        return Err("Usage: cargo run --example watch -- <pid> [seconds]".into());
    }
    let pid = args[0].parse::<u32>()?;
    let duration = args
        .get(1)
        .map(|v| v.parse::<u64>())
        .transpose()?
        .unwrap_or(0);
    if pid == 0 || duration > 7200 {
        return Err("Invalid PID or duration".into());
    }
    let start = Instant::now();
    loop {
        let group = openmemduct_lib::diagnostics::process_group(pid)?;
        println!("{}", serde_json::to_string(&group)?);
        if start.elapsed() >= Duration::from_secs(duration) {
            break;
        }
        std::thread::sleep(
            Duration::from_secs(5)
                .min(Duration::from_secs(duration).saturating_sub(start.elapsed())),
        );
    }
    Ok(())
}
