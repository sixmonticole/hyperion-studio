//! Prints the memory figures the guard uses, to check them against `vm_stat`.
//!
//! Usage: diag_memory_info

fn main() {
    let total = hyperion_studio_lib::pipeline::cache::total_memory();
    let avail = hyperion_studio_lib::pipeline::cache::available_memory();
    let gb = |b: Option<u64>| match b {
        Some(v) => format!("{:.2} GB", v as f64 / 1024.0 / 1024.0 / 1024.0),
        None => "unavailable".into(),
    };
    println!("total      = {}", gb(total));
    println!("available  = {}", gb(avail));

    // The estimate for a real 3620x3620 crop.
    let px = 3620usize * 3620;
    let need = hyperion_studio_lib::pipeline::estimate_peak_memory(200, px, 10);
    println!("estimate   = {}", gb(Some(need)));

    // Mirror the UI rule: refuse only when the run cannot fit in physical
    // memory at all; warn when it only exceeds what is free right now.
    match (total, avail) {
        (Some(t), a) => {
            if need > t {
                println!("guard says = REFUSE (exceeds total physical memory)");
            } else if a.is_some_and(|a| need > a) {
                println!("guard says = WARN (over current free memory, will swap)");
            } else {
                println!("guard says = allow");
            }
        }
        _ => println!("guard says = allow (no measurement)"),
    }
}
