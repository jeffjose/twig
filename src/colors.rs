use colored::*;

pub fn print_color_test() {
    println!("\nAvailable colors and styles:\n");

    // Basic ANSI colors (4-bit)
    println!("Basic ANSI colors (4-bit):");
    let colors = [
        ("red", "█████".red()),
        ("green", "█████".green()),
        ("yellow", "█████".yellow()),
        ("blue", "█████".blue()),
        ("magenta", "█████".magenta()),
        ("cyan", "█████".cyan()),
        ("white", "█████".white()),
    ];

    for (name, color) in colors.iter() {
        print!("{}: {} ", name, color);
    }
    println!("\n");

    // Bright ANSI colors
    println!("Bright ANSI colors:");
    let bright_colors = [
        ("bright_red", "█████".bright_red()),
        ("bright_green", "█████".bright_green()),
        ("bright_yellow", "█████".bright_yellow()),
        ("bright_blue", "█████".bright_blue()),
        ("bright_magenta", "█████".bright_magenta()),
        ("bright_cyan", "█████".bright_cyan()),
        ("bright_white", "█████".bright_white()),
    ];

    for (name, color) in bright_colors.iter() {
        print!("{}: {} ", name, color);
    }
    println!("\n");

    // 256 color mode (8-bit)
    println!("256 Color mode (8-bit) - Sample:");
    for i in 0..16 {
        for j in 0..16 {
            let color = i * 16 + j;
            print!("\x1b[48;5;{}m  \x1b[0m", color);
        }
        println!();
    }
    println!();

    // True color mode (24-bit) - RGB
    println!("True color mode (24-bit) - Sample gradient:");
    for i in 0..24 {
        for j in 0..72 {
            let r = (i as f32 * 10.0) as u8;
            let g = (j as f32 * 3.5) as u8;
            let b = 128;
            print!("\x1b[48;2;{};{};{}m  \x1b[0m", r, g, b);
        }
        println!();
    }
    println!();

    // Styles
    println!("Styles:");
    println!("normal: {}", "Hello World".normal());
    println!("bold: {}", "Hello World".bold());
    println!("italic: {}", "Hello World".italic());
    println!();

    // Combinations
    println!("Example combinations:");
    println!("bold+red: {}", "Hello World".red().bold());
    println!("italic+blue: {}", "Hello World".blue().italic());
    println!("bold+bright_green: {}", "Hello World".bright_green().bold());
} 
