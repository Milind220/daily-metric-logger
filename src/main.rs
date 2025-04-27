use chrono::{DateTime, NaiveDate, Utc};
use colored::*; // Import colored text features
use csv::{ReaderBuilder, StringRecord, WriterBuilder};
use dialoguer::{theme::ColorfulTheme, Confirm, Input};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fs::{File, OpenOptions};
use std::path::Path; // To initialize theme easily

const DATA_FILE: &str = "daily_metrics.csv";
const GOAL_DAYS: i64 = 30;

// --- Define the structure for our log entry ---
#[derive(Debug, Serialize, Deserialize)]
struct LogEntry {
    timestamp: String, // Store as ISO 8601 string for simplicity in CSV
    day_count: i64,
    sleep_hours: Option<f32>, // Optional because it's asked only once a day
    sleep_quality: Option<f32>, // Optional because it's asked only once a day
    sleepiness: u8,
    zonkedness: u8,
    energy: u8,
    strength: u8,
    focus: u8,
    intelligence: u8,
    workout_today: bool,
    remarks: String,
}

// --- Define a custom error type ---
#[derive(thiserror::Error, Debug)]
enum AppError {
    #[error("CSV processing error: {0}")]
    CsvError(#[from] csv::Error),
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Date parsing error: {0}")]
    DateParseError(#[from] chrono::ParseError),
    #[error("Dialog interaction cancelled")]
    DialogCancelled, // New variant for cancellation
}

// --- Helper struct to store info from existing CSV ---
struct CsvInfo {
    first_entry_date: Option<NaiveDate>,
    last_entry_date: Option<NaiveDate>,
    workout_logged_today: bool,
}

// --- Initialize the theme once ---
lazy_static! {
    static ref THEME: ColorfulTheme = ColorfulTheme::default();
}

// --- Input Validation ---

// --- Main Application Logic ---
fn main() -> Result<(), Box<dyn Error>> {
    println!("{}", "=".repeat(40).cyan());
    println!("{}", " Daily Metrics Logger ".bold().cyan());
    println!("{}", "=".repeat(40).cyan());

    let csv_info = read_csv_info(DATA_FILE)?;
    let today = Utc::now().date_naive();

    // Determine if it's the first entry of the day
    let is_first_entry_today = match csv_info.last_entry_date {
        Some(last_date) => last_date != today,
        None => true, // No previous entries means this is the first
    };

    // Determine the first ever entry date (or today if none)
    let first_ever_date = csv_info.first_entry_date.unwrap_or(today);

    // Calculate day count
    let day_count = (today - first_ever_date).num_days() + 1; // +1 because day 1 is the first day

    println!("Current Date: {}", today.format("%Y-%m-%d"));
    println!(
        "Logging Day: {} / {} (Goal)",
        day_count.to_string().yellow(),
        GOAL_DAYS.to_string().green()
    );
    println!("{}", "-".repeat(40).cyan());

    // --- Collect Data ---
    let mut sleep_hours: Option<f32> = None;
    let mut sleep_quality: Option<f32> = None;
    if is_first_entry_today {
        println!("{}", "First log of the day!".bright_blue());
        sleep_hours = Some(
            Input::with_theme(&*THEME)
                .with_prompt("How many hours did you sleep last night?")
                .validate_with(|input: &String| -> Result<(), String> {
                    match input.parse::<f32>() {
                        Ok(val) => {
                            if val <= 12.0 {
                                // Max 12 hours, min is implicitly 0 for u8
                                Ok(())
                            } else {
                                Err("Please enter a number between 0 and 12".to_string())
                            }
                        }
                        Err(_) => Err("Please enter a valid number".to_string()),
                    }
                })
                .default("8".to_string()) // Sensible default
                .interact_text()
                .map_err(|_| AppError::DialogCancelled)? // Handle potential cancel
                .parse::<f32>()?, // Parse validated input
        );
        sleep_quality = Some(
            Input::with_theme(&*THEME)
                .with_prompt("Rate sleep quality (1.0=Poor, 10.0=Excellent)")
                .validate_with(|input: &String| -> Result<(), String> {
                    match input.parse::<f32>() {
                        Ok(val) => {
                            if (1.0..=10.0).contains(&val) {
                                Ok(())
                            } else {
                                Err("Please enter a value between 1.0 and 10.0".to_string())
                            }
                        }
                        Err(_) => Err("Please enter a valid float (e.g. 7.5)".to_string()),
                    }
                })
                .default("7.5".to_string())
                .interact_text()
                .map_err(|_| AppError::DialogCancelled)?
                .parse::<f32>()?, // Parse validated input
        );
    } else {
        println!("{}", "Follow-up log for today.".dimmed());
    }

    let sleepiness = ask_rating("Sleepiness/Grogginess (1=Low, 10=High)")?;
    let zonkedness = ask_rating("Zonked-ness (1=Low, 10=High)")?;
    let energy = ask_rating("Energy Levels (1=Low, 10=High)")?;
    let strength = ask_rating("Physical Strength (1=Low, 10=High)")?;
    let focus = ask_rating("Focus (1=Low, 10=High)")?;
    let intelligence = ask_rating("Perceived Intelligence (1=Low, 10=High)")?; // Wording change for clarity

    let workout_today: bool;

    if !csv_info.workout_logged_today {
        // Only ask if no 'yes' workout has been logged today yet
        println!("{}", "Checking workout status...".blue()); // Info message
        workout_today = Confirm::with_theme(&*THEME)
            .with_prompt("Did you (or will you) workout today?")
            .interact()
            .map_err(|_| AppError::DialogCancelled)?; // Handle potential cancel
        if workout_today {
            println!("{}", " -> Awesome!".yellow());
        } else {
            println!("{}", " -> Ok, maybe later.".dimmed());
        }
    } else {
        // A 'yes' was already logged today, so don't ask again.
        println!(
            "{}",
            "Workout already logged as 'yes' earlier today.".dimmed()
        );
        workout_today = true; // Assume 'true' for this follow-up entry as well
    }

    let remarks: String = Input::with_theme(&*THEME)
        .with_prompt("Any remarks?")
        .allow_empty(true) // Allow empty remarks
        .interact_text()
        .map_err(|_| AppError::DialogCancelled)?; // Handle potential cancel

    let timestamp = Utc::now(); // Record time after all questions are answered

    // --- Create Log Entry ---
    let entry = LogEntry {
        timestamp: timestamp.to_rfc3339(), // ISO 8601 format
        day_count,
        sleep_hours,
        sleep_quality,
        sleepiness,
        zonkedness,
        energy,
        strength,
        focus,
        intelligence,
        workout_today,
        remarks,
    };

    // --- Write to CSV ---
    append_to_csv(DATA_FILE, &entry)?;

    println!("{}", "\n----------------------------------------".green());
    println!("{}", " Entry successfully logged!".bold().green());
    println!(
        " Timestamp: {}",
        timestamp
            .format("%Y-%m-%d %H:%M:%S %Z")
            .to_string()
            .dimmed()
    );
    println!("{}", "----------------------------------------".green());

    Ok(())
}

// --- Helper function to ask for a 1-10 rating ---
fn ask_rating(prompt: &str) -> Result<u8, AppError> {
    Input::with_theme(&*THEME)
        .with_prompt(prompt)
        .validate_with(|input: &String| -> Result<(), String> {
            match input.parse::<u8>() {
                Ok(val) => {
                    if (1..=10).contains(&val) {
                        Ok(())
                    } else {
                        Err("Please enter a number between 1 and 10".to_string())
                    }
                }
                Err(_) => Err("Please enter a valid number".to_string()),
            }
        })
        .interact_text()
        .map_err(|_| AppError::DialogCancelled)? // Handle potential cancel
        .parse::<u8>() // We know it's valid u8 due to validator
        .map_err(|e| AppError::IoError(std::io::Error::new(std::io::ErrorKind::InvalidInput, e)))
}

// --- Helper function to read first and last date from CSV ---
fn read_csv_info(file_path: &str) -> Result<CsvInfo, AppError> {
    let mut first_date: Option<NaiveDate> = None;
    let mut last_date: Option<NaiveDate> = None;
    let mut workout_today_logged = false; // Initialize flag for the new logic

    let today = Utc::now().date_naive(); // Get today's date once

    if Path::new(file_path).exists() {
        let file = File::open(file_path)?;
        let mut rdr = ReaderBuilder::new()
            .has_headers(true) // Expect a header row
            .from_reader(file);

        // Define the expected header name for robustness check
        const TIMESTAMP_HEADER: &str = "timestamp";
        // Define the column index for workout_today (0-based)
        const WORKOUT_COLUMN_INDEX: usize = 10;

        for result in rdr.records() {
            let record = match result {
                Ok(rec) => rec,
                Err(e) => {
                    eprintln!("Warning: Skipping corrupted CSV record: {}", e);
                    continue; // Skip this record
                }
            };

            // Get timestamp string
            if let Some(ts_str) = record.get(0) {
                // Safety check: Skip if we somehow got the header row
                if ts_str.trim() == TIMESTAMP_HEADER {
                    eprintln!("Warning: Skipping potential header row accidentally read as data.");
                    continue;
                }

                // Parse timestamp
                let dt = DateTime::parse_from_rfc3339(ts_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .map_err(|e| {
                        eprintln!(
                            "Warning: Could not parse timestamp '{}' in data row: {}. Skipping record.", // Changed log level
                            ts_str, e
                        );
                        // Don't return AppError here, just skip the record for date finding purposes
                        // AppError::DateParseError(e)
                    });

                // Proceed only if date parsing was successful
                if let Ok(dt) = dt {
                    let current_date = dt.date_naive();

                    // Update first date logic
                    if first_date.is_none() || current_date < first_date.unwrap() {
                        first_date = Some(current_date);
                    }
                    // Update last date (always override with the latest processed valid record)
                    last_date = Some(current_date);

                    // --- New logic: Check workout status for today's entries ---
                    if current_date == today {
                        if let Some(workout_str) = record.get(WORKOUT_COLUMN_INDEX) {
                            // Check if workout was logged as 'true' case-insensitively
                            if workout_str.trim().eq_ignore_ascii_case("true") {
                                workout_today_logged = true;
                                // Optimization note: We could potentially break early if we only needed this flag,
                                // but we still need to loop through all records to find the *last* date reliably.
                            }
                        } else {
                            eprintln!("Warning: Record for today ({}) is missing workout column (index {}).", current_date, WORKOUT_COLUMN_INDEX);
                        }
                    }
                    // --- End of new logic ---
                } // End if Ok(dt)
            } else {
                eprintln!("Warning: Skipping record with missing timestamp column.");
            }
        } // End for loop
    }

    Ok(CsvInfo {
        first_entry_date: first_date,
        last_entry_date: last_date,
        workout_logged_today: workout_today_logged, // Return the determined flag
    })
}

// --- Helper function to append data to CSV ---
fn append_to_csv(file_path: &str, entry: &LogEntry) -> Result<(), AppError> {
    let file_exists = Path::new(file_path).exists();

    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(file_path)?;

    let mut wtr = WriterBuilder::new()
        .has_headers(!file_exists) // Write headers only if file is new
        .from_writer(file);

    // Write header if it's a new file
    if !file_exists {
        // Manually create header record from struct field names
        // Note: Order must match LogEntry struct fields for clarity, though serde handles it
        let headers = StringRecord::from(vec![
            "timestamp",
            "day_count",
            "sleep_hours",
            "sleep_quality",
            "sleepiness",
            "zonkedness",
            "energy",
            "strength",
            "focus",
            "intelligence",
            "workout_today",
            "remarks",
        ]);
        wtr.write_record(&headers)?;
    }

    // Serialize and write the data record
    wtr.serialize(entry)?;
    wtr.flush()?; // Ensure data is written to disk
    Ok(())
}
