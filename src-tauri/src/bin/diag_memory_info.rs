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
    match avail {
        Some(a) => println!(
            "guard says = {}",
            if need > a { "REFUSE" } else { "allow" }
        ),
        None => println!("guard says = allow (no measurement)"),
    }
}
