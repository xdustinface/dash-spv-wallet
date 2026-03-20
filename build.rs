use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=input.css");
    println!("cargo:rerun-if-changed=src/");

    // Try to compile Tailwind CSS. If npx is not available, fall back
    // to using the existing assets/tailwind.css (if any).
    let result = Command::new("npx")
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
            eprintln!(
                "cargo:warning=Tailwind CSS compilation failed (exit code: {}). \
                 Styles may be missing. Run `npm install` to set up Tailwind.",
                status
            );
        }
        Err(_) => {
            eprintln!(
                "cargo:warning=`npx` not found. Tailwind CSS will not be compiled. \
                 Install Node.js and run `npm install` for styled builds."
            );
        }
    }
}
