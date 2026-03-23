use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=input.css");
    println!("cargo:rerun-if-changed=src/");

    // On Windows, npx is a .cmd script
    let npx = if cfg!(windows) { "npx.cmd" } else { "npx" };

    let result = Command::new(npx)
        .args([
            "@tailwindcss/cli",
            "-i",
            "./input.css",
            "-o",
            "./assets/tailwind.css",
            "--minify",
        ])
        .status();

    match result {
        Ok(status) if status.success() => {}
        Ok(status) => {
            println!(
                "cargo:warning=Tailwind CSS compilation failed (exit code: {status}). \
                 Run `npm install` to set up Tailwind.",
            );
        }
        Err(_) => {
            println!(
                "cargo:warning=`npx` not found. Run `npm install` and ensure Node.js is installed.",
            );
        }
    }
}
