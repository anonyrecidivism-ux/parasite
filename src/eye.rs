use std::time::Duration;
use tokio_util::sync::CancellationToken;

const E:   &str = "\x1b[31m";       // red   — eyelid
const DR:  &str = "\x1b[38;5;88m";  // dark red — veins
const G:   &str = "\x1b[90m";       // gray  — iris frame
const RST: &str = "\x1b[0m";

pub const EYE_LINES: usize = 9;

const IW: usize = 20; // iris interior width

// (px, psize, squint, speed_ms)
const SEQUENCE: &[(i32, u8, u8, u64)] = &[
    ( 0, 2, 0, 340),( 0, 2, 0, 280),
    ( 5, 2, 0,  65),( 9, 3, 0, 110),( 4, 2, 0,  65),( 0, 2, 0,  90),
    (-5, 2, 0,  65),(-9, 3, 0, 110),(-4, 2, 0,  65),( 0, 2, 0,  90),
    ( 3, 2, 0,  40),(-3, 2, 0,  40),( 3, 2, 0,  40),(-3, 2, 0,  40),
    ( 1, 2, 0,  40),(-1, 2, 0,  40),( 0, 2, 0, 110),
    ( 0, 4, 0, 230),( 0, 1, 0,  90),( 0, 4, 0,  90),( 0, 1, 0,  90),
    ( 0, 4, 0, 140),( 0, 2, 0, 110),
    ( 0, 2, 1, 160),( 0, 2, 0,  80),( 0, 2, 1,  80),( 0, 2, 0,  80),
    ( 0, 2, 1, 110),( 0, 2, 0,  90),
    ( 9, 2, 0,  50),( 6, 3, 1,  50),( 0, 2, 0,  50),(-6, 3, 1,  50),
    (-9, 2, 0,  50),( 0, 1, 0,  50),( 0, 4, 0,  50),( 9, 4, 0,  50),
    (-9, 4, 0,  50),( 0, 1, 0,  50),
    ( 4, 2, 1,  50),(-4, 2, 1,  50),( 4, 2, 1,  50),(-4, 2, 1,  50),
    ( 0, 3, 0,  50),( 0, 1, 0,  50),( 0, 3, 0,  50),( 0, 2, 0, 240),
];

// vein left / vein right — 6 chars each
const VL: &[&str] = &["·╱─·─·","─·╱·──","╱·─·──","·─·╱·─","·──╱·─","──·╱·─","·─·╱──","·╱·──·"];
const VR: &[&str] = &["·─·╲·─","──·╲·─","─·──·╲","─·╲·──","─·╲──·","─·╲·──","──·╲·─","·──·╲·"];

fn shade(n: usize, left: bool) -> String {
    (0..n).map(|i| {
        let t = if n == 0 { 0.5 } else { i as f64 / n as f64 };
        let t = if left { t } else { 1.0 - t };
        if t < 0.35 { '░' } else if t < 0.70 { '▒' } else { '▓' }
    }).collect()
}

fn iris_row(pos: usize, pw: usize, show_pupil: bool) -> String {
    let left  = pos;
    let right = IW - pos - pw;
    let p = if show_pupil { "█".repeat(pw) } else { "▒".repeat(pw) };
    format!("{}{}{}", shade(left, true), p, shade(right, false))
}

pub fn build_frame(px: i32, psize: u8, squint: u8, vi: usize) -> Vec<String> {
    let pw: usize = match psize {
        1 => 3, 2 => 5, 3 => 7, 4 => 11, _ => 5,
    }.min(IW - 2);

    let center = ((IW - pw) / 2) as i32;
    let pos    = (center + px).clamp(0, (IW - pw) as i32) as usize;

    let vl = VL[vi % VL.len()];
    let vr = VR[vi % VR.len()];

    let itop = format!("{G}╔{}{G}╗{RST}", "═".repeat(IW));
    let ibot = format!("{G}╚{}{G}╝{RST}", "═".repeat(IW));

    let sh = iris_row(pos, pw, false); // shade-only row
    let pp = iris_row(pos, pw, true);  // pupil row

    let ir = |content: &str| -> String {
        format!("{G}║{RST}{}{G}║{RST}", content)
    };

    match squint {
        0 => vec![
            format!("              {E}╭{}╮{RST}              ", "─".repeat(IW + 8)),
            format!("      {E}╭─────╯{RST}{DR}{vl}{RST} {itop} {DR}{vr}{RST}{E}╰─────╮{RST}      "),
            format!("    {E}╭─╯{RST}  {DR}╱{RST}  {}  {DR}╲{RST}  {E}╰─╮{RST}    ", ir(&sh)),
            format!("   {E}╭╯{RST}   {DR}│{RST}  {}  {DR}│{RST}   {E}╰╮{RST}   ", ir(&pp)),
            format!("   {E}│{RST}    {DR}│{RST}  {}  {DR}│{RST}    {E}│{RST}   ", ir(&pp)),
            format!("   {E}╰╮{RST}   {DR}│{RST}  {}  {DR}│{RST}   {E}╭╯{RST}   ", ir(&pp)),
            format!("    {E}╰─╮{RST}  {DR}╲{RST}  {}  {DR}╱{RST}  {E}╭─╯{RST}    ", ir(&sh)),
            format!("      {E}╰─────╮{RST}{DR}{vl}{RST} {ibot} {DR}{vr}{RST}{E}╭─────╯{RST}      "),
            format!("              {E}╰{}╯{RST}              ", "─".repeat(IW + 8)),
        ],
        1 => vec![
            format!("              {E}╭{}╮{RST}              ", "─".repeat(IW + 8)),
            format!("      {E}╭─────╯{RST}{DR}{vl}{RST} {itop} {DR}{vr}{RST}{E}╰─────╮{RST}      "),
            format!("    {E}╭─────────────────────────────────────────────────╮{RST}"),
            format!("   {E}╰╮{RST}   {DR}│{RST}  {}  {DR}│{RST}   {E}╭╯{RST}   ", ir(&pp)),
            format!("    {E}╰─────────────────────────────────────────────────╯{RST}"),
            format!("      {E}╰─────╮{RST}{DR}{vl}{RST} {ibot} {DR}{vr}{RST}{E}╭─────╯{RST}      "),
            format!("              {E}╰{}╯{RST}              ", "─".repeat(IW + 8)),
            String::new(),
            String::new(),
        ],
        _ => vec![
            format!("              {E}╭{}╮{RST}              ", "─".repeat(IW + 8)),
            format!("      {E}╭─────╯{RST}{DR}{vl}{RST} {itop} {DR}{vr}{RST}{E}╰─────╮{RST}      "),
            format!("    {E}╭─────────────────────────────────────────────────╮{RST}"),
            format!("    {E}╰─────────────────────────────────────────────────╯{RST}"),
            format!("      {E}╰─────╮{RST}{DR}{vl}{RST} {ibot} {DR}{vr}{RST}{E}╭─────╯{RST}      "),
            format!("              {E}╰{}╯{RST}              ", "─".repeat(IW + 8)),
            String::new(),
            String::new(),
            String::new(),
        ],
    }
}

pub async fn run_animation(cancel: CancellationToken) {
    let first = build_frame(0, 2, 0, 0);
    for line in &first { println!("{line}"); }
    crate::ui::flush();

    let mut seq_idx = 0usize;
    let mut vi      = 0usize;

    loop {
        let (px, psize, squint, speed_ms) = SEQUENCE[seq_idx % SEQUENCE.len()];
        seq_idx += 1;
        if seq_idx % 3 == 0 { vi = vi.wrapping_add(1); }

        tokio::select! {
            _ = cancel.cancelled() => break,
            _ = tokio::time::sleep(Duration::from_millis(speed_ms)) => {}
        }

        crate::ui::cursor_up(EYE_LINES);
        for line in build_frame(px, psize, squint, vi) { println!("{line}"); }
        crate::ui::flush();
    }

    // закрыть глаз
    for sq in [1u8, 2, 2] {
        crate::ui::cursor_up(EYE_LINES);
        for line in build_frame(0, 2, sq, vi) { println!("{line}"); }
        crate::ui::flush();
        tokio::time::sleep(Duration::from_millis(80)).await;
    }

    // стереть глаз с экрана
    crate::ui::cursor_up(EYE_LINES);
    for _ in 0..EYE_LINES {
        println!("{}", " ".repeat(70));
    }
    crate::ui::cursor_up(EYE_LINES);
}
