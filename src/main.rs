use std::fs::OpenOptions;
use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::io::AsRawFd;
use std::process::{exit, Command};
use std::thread;
use std::time::Duration;

// === Configuration Constants ===

/// Enable debug output for troubleshooting
const DEBUG_MODE: bool = true;

/// Paths and directories
const TERMINAL_DEVICE_PATH: &str = "/dev/tty0";
const LOG_FILE_PATH: &str = "/mnt/log.txt";
const CONSOLE_DEVICE_PATH: &str = "/dev/tty0";
const DEFAULT_BINARY_PATH: &str = "/bin";
const MOUNT_POINT_PATH: &str = "/mnt";
const LOG_STORAGE_DEVICE_PATH: &str = "/dev/sda1";
const GRAPHICS_DEVICE_PATH: &str = "/dev/dri/card0";

/// Browser configuration
const BROWSER_EXECUTABLE_PATH: &str = "/usr/bin/ui/content_shell";
const BROWSER_DEFAULT_URL: &str = "http://www.example.com";

/// Environment variables for browser and system
const EGL_DEBUG_VALUE: &str = "1";
const EGL_LOG_LEVEL_VALUE: &str = "debug";
const LIBGL_ALWAYS_SOFTWARE_VALUE: &str = "0";
const LIBRARY_PATH_VALUE: &str = "/lib:/usr/lib:/lib64:/usr/lib/x86_64-linux-gnu";
const SYSTEM_PATH_VALUE: &str = "/bin:/usr/bin";

/// Timing constants
const SYNC_INTERVAL_SECONDS: u64 = 2;
const RETRY_DELAY_SECONDS: u64 = 2;

/// File descriptor constants
const STDOUT_FILE_DESCRIPTOR: i32 = 1;
const STDERR_FILE_DESCRIPTOR: i32 = 2;

// === Output and Utility Functions ===

/// Output a line to stdout with immediate flush
fn output_line(message: &str) {
    println!("{}", message);
    io::stdout().flush().unwrap_or(());
}

/// Output text to stdout without newline and immediate flush
fn output(message: &str) {
    print!("{}", message);
    io::stdout().flush().unwrap_or(());
}

/// Sleep for the specified number of seconds
fn sleep_seconds(seconds: u64) {
    thread::sleep(Duration::from_secs(seconds));
}

// === Output Redirection Functions ===

/// Redirect stdout and stderr to the system terminal
/// This function retries indefinitely until successful
fn redirect_output_to_terminal() -> io::Result<()> {
    loop {
        match OpenOptions::new().write(true).open(TERMINAL_DEVICE_PATH) {
            Ok(terminal_file) => {
                output_line("Redirecting output to the terminal...");

                let terminal_file_descriptor = terminal_file.as_raw_fd();
                unsafe {
                    libc::dup2(terminal_file_descriptor, STDOUT_FILE_DESCRIPTOR);
                    libc::dup2(terminal_file_descriptor, STDERR_FILE_DESCRIPTOR);
                }
                break;
            }
            Err(_) => {
                output_line("Failed to redirect output to the terminal");
                sleep_seconds(RETRY_DELAY_SECONDS);
            }
        }
    }
    Ok(())
}

/// Redirect stdout and stderr to a log file
/// This function retries indefinitely until successful
fn redirect_output_to_log_file() -> io::Result<()> {
    loop {
        match OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(LOG_FILE_PATH)
        {
            Ok(log_file) => {
                output_line("Redirecting output to a log file!");

                let log_file_descriptor = log_file.as_raw_fd();
                unsafe {
                    libc::dup2(log_file_descriptor, STDOUT_FILE_DESCRIPTOR);
                    libc::dup2(log_file_descriptor, STDERR_FILE_DESCRIPTOR);
                }
                break;
            }
            Err(_) => {
                output_line("Failed to redirect to a log file!");
                sleep_seconds(RETRY_DELAY_SECONDS);
            }
        }
    }
    Ok(())
}

// === Shell Command Execution ===

/// Execute a shell command with proper environment setup
/// Commands are parsed to handle quoted arguments and are executed with
/// full environment variables configured for browser operation
fn execute_shell_command(command: &str) -> io::Result<()> {
    let command_parts = parse_shell_command(command);

    if command_parts.is_empty() {
        return Ok(());
    }

    let mut full_command_path = command_parts[0].clone();

    // If command doesn't start with '/', prepend the default binary path
    if !full_command_path.starts_with('/') {
        full_command_path = format!("{}/{}", DEFAULT_BINARY_PATH, full_command_path);
    }

    // Debug output to show command execution
    if DEBUG_MODE {
        print!("$ {}", full_command_path);
        for argument in &command_parts[1..] {
            print!(" \"{}\"", argument);
        }
        println!();
    }

    let mut shell_command = Command::new(&full_command_path);
    if command_parts.len() > 1 {
        shell_command.args(&command_parts[1..]);
    }

    // Configure environment variables for proper system and browser operation
    configure_command_environment(&mut shell_command);

    match shell_command.status() {
        Ok(exit_status) => {
            if !exit_status.success() {
                output_line(&format!(
                    "Command failed with exit code: {:?}",
                    exit_status.code()
                ));
            }
        }
        Err(execution_error) => {
            output_line(&format!("Failed to execute command: {}", execution_error));
        }
    }

    Ok(())
}

/// Configure environment variables for a command
/// Sets up EGL, graphics, and path variables needed for browser operation
fn configure_command_environment(command: &mut Command) {
    command
        .env("EGL_DEBUG", EGL_DEBUG_VALUE)
        .env("EGL_LOG_LEVEL", EGL_LOG_LEVEL_VALUE)
        .env("DRI_DEVICE", GRAPHICS_DEVICE_PATH)
        .env("LIBGL_ALWAYS_SOFTWARE", LIBGL_ALWAYS_SOFTWARE_VALUE)
        .env("LD_LIBRARY_PATH", LIBRARY_PATH_VALUE)
        .env("PATH", SYSTEM_PATH_VALUE);
}

/// Parse a shell command string into individual arguments
/// Handles quoted strings properly to preserve spaces within arguments
fn parse_shell_command(command: &str) -> Vec<String> {
    let mut arguments = Vec::new();
    let mut current_argument = String::new();
    let mut inside_quotes = false;
    let mut character_iterator = command.chars().peekable();

    while let Some(character) = character_iterator.next() {
        match character {
            '"' => {
                inside_quotes = !inside_quotes;
            }
            ' ' if !inside_quotes => {
                if !current_argument.is_empty() {
                    arguments.push(current_argument.clone());
                    current_argument.clear();
                }
            }
            _ => {
                current_argument.push(character);
            }
        }
    }

    if !current_argument.is_empty() {
        arguments.push(current_argument);
    }

    arguments
}

// === System Setup Functions ===

/// Start a background process that syncs filesystem data periodically
/// This helps prevent data loss in case of unexpected shutdown
fn start_sync_process() -> io::Result<()> {
    match unsafe { libc::fork() } {
        0 => {
            // Child process - run sync every few seconds
            loop {
                sleep_seconds(SYNC_INTERVAL_SECONDS);
                let _ = execute_shell_command("sync");
            }
        }
        -1 => {
            output_line("Failed to fork sync process");
            return Err(io::Error::last_os_error());
        }
        _process_id => {
            // Parent process continues
            output_line("Started background sync process");
        }
    }

    Ok(())
}

/// Create symbolic links for standard system directories
/// This provides compatibility with programs expecting standard paths
fn create_symbolic_links() -> io::Result<()> {
    output_line("Creating symbolic links...");

    let symbolic_links = [
        ("/usr/bin", "/bin"),
        ("/usr/lib", "/lib"),
        ("/usr/lib64", "/lib64"),
    ];

    for (target_path, link_path) in &symbolic_links {
        match std::os::unix::fs::symlink(target_path, link_path) {
            Ok(()) => {
                output_line(&format!("Created {} -> {}", link_path, target_path));
            }
            Err(_) => {
                output_line(&format!(
                    "Failed to create {} -> {}",
                    link_path, target_path
                ));
            }
        }
    }

    Ok(())
}

/// Mount essential system filesystems
/// Sets up proc, dev, and sys filesystems needed for system operation
fn mount_filesystems() -> io::Result<()> {
    let mount_commands = [
        "mkdir -p /proc",
        "mkdir -p /dev",
        "mount -t proc none /proc",
        "mount -t devtmpfs devtmpfs /dev",
        "mount -t sysfs sysfs /sys",
    ];

    for mount_command in &mount_commands {
        execute_shell_command(mount_command)?;
    }

    Ok(())
}

/// Setup temporary filesystems for shared memory and temporary files
/// Configures appropriate permissions and ownership for browser operation
fn setup_temporary_filesystems() -> io::Result<()> {
    let temporary_filesystem_commands = [
        "mkdir -p /dev/shm",
        "mount -t tmpfs -o nosuid,nodev,uid=1000,gid=1000,mode=0777 shmfs /dev/shm",
        "mount -t tmpfs -o uid=1000,gid=1000,mode=0777 tmpfs /tmp",
    ];

    for temp_command in &temporary_filesystem_commands {
        execute_shell_command(temp_command)?;
    }

    Ok(())
}

/// Mount persistent log storage device
/// Waits for the log storage device to become available and mounts it
/// This provides persistent storage for log files
fn mount_log_storage() -> io::Result<()> {
    output_line("Waiting for log storage device to become available...");
    
    // Wait for the log storage device to become available
    loop {
        if std::path::Path::new(LOG_STORAGE_DEVICE_PATH).exists() {
            output_line("Log storage device detected, proceeding with mount...");
            break;
        }
        output_line("Log storage device not ready, waiting...");
        sleep_seconds(RETRY_DELAY_SECONDS);
    }

    let storage_commands = [
        &format!("mkdir -p {}", MOUNT_POINT_PATH),
        &format!("mount {} {}", LOG_STORAGE_DEVICE_PATH, MOUNT_POINT_PATH),
    ];

    for storage_command in &storage_commands {
        execute_shell_command(storage_command)?;
    }

    output_line("Log storage device mounted successfully");
    Ok(())
}

// === Browser Configuration and Launch ===

/// Browser command line arguments for optimal performance and security
const BROWSER_ARGUMENTS: &[&str] = &[
    "--no-sandbox",     // Disable sandbox for embedded systems
    "--in-process-gpu", // Run GPU process in main process
    "--single-process", // Use single process mode for stability
    "--ozone-platform=drm", // Use DRM platform for hardware acceleration
    "--content-shell-hide-toolbar", // We do not want the toolbar
];

/// Start the web browser with robust configuration
/// Attempts to launch the browser with comprehensive arguments for embedded systems
fn start_browser() -> io::Result<()> {
    output_line("Starting web browser...");

    // Build the complete browser command
    let mut browser_command = String::from(BROWSER_EXECUTABLE_PATH);

    // Add all browser arguments
    for argument in BROWSER_ARGUMENTS {
        browser_command.push(' ');
        browser_command.push_str(argument);
    }

    // Add the target URL
    browser_command.push(' ');
    browser_command.push_str(BROWSER_DEFAULT_URL);

    // Execute the browser command
    execute_shell_command(&browser_command)
}

// === Interactive Shell ===

/// Provide an interactive shell for user commands
/// Reads commands from the console and executes them using the shell command processor
fn interactive_shell() -> io::Result<()> {
    output_line("Starting interactive shell. Type commands or Ctrl+C to exit.");

    loop {
        output("$ ");

        let console_file = match OpenOptions::new().read(true).open(CONSOLE_DEVICE_PATH) {
            Ok(file_handle) => file_handle,
            Err(_) => {
                output_line("Failed to open console for input");
                break;
            }
        };

        let mut console_reader = BufReader::new(console_file);
        let mut user_command = String::new();

        match console_reader.read_line(&mut user_command) {
            Ok(0) => break, // End of file reached
            Ok(_) => {
                let trimmed_command = user_command.trim();
                if !trimmed_command.is_empty() {
                    if trimmed_command == "exit" || trimmed_command == "quit" {
                        output_line("Exiting interactive shell...");
                        break;
                    }
                    let _ = execute_shell_command(trimmed_command);
                }
            }
            Err(_) => {
                output_line("Failed to read command from console");
                break;
            }
        }
    }

    Ok(())
}

// === Main Initialization Function ===

/// Main initialization sequence
/// Orchestrates the complete system setup process in the correct order
fn run_initialization() -> io::Result<()> {
    output_line("Starting webgate initializer...");
    output_line("Initializing system components...");

    output_line("[1/9]: Setting up symbolic links");
    create_symbolic_links()?;

    output_line("[2/9]: Mounting basic filesystems");
    mount_filesystems()?;

    output_line("[3/9]: Starting background sync process");
    start_sync_process()?;

    output_line("[4/9]: Configuring output redirection");
    redirect_output_to_terminal()?;

    output_line("[5/9]: Setting up temporary filesystems");
    setup_temporary_filesystems()?;

    output_line("[6/9]: Mounting log storage device");
    if DEBUG_MODE {
        mount_log_storage()?;
    }

    output_line("[7/9]: Setting up logging");
    if DEBUG_MODE {
        redirect_output_to_log_file()?;
    }

    output_line("[8/9]: Launching web browser");
    start_browser()?;

    output_line("[9/9]: Starting interactive shell");
    interactive_shell()?;

    output_line("Initialization sequence completed successfully");
    Ok(())
}

/// Main entry point
/// Handles the initialization process and provides appropriate exit codes
fn main() {
    output_line("Webgate System Initializer v1.0");
    output_line("==========================================");

    match run_initialization() {
        Ok(()) => {
            output_line("System initialization completed successfully");
            exit(0);
        }
        Err(initialization_error) => {
            output_line(&format!(
                "System initialization failed: {}",
                initialization_error
            ));
            output_line("Check system configuration and try again");
            exit(1);
        }
    }
}
