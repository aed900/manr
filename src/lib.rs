use std::{env, fs, fs::File, error::Error, path::Path, path::PathBuf, process, process::Command, process::Stdio, 
    io, io::prelude::*, io::Write, io::BufRead, io::BufReader, io::BufWriter, io::ErrorKind, collections::HashMap};
use walkdir::{DirEntry, WalkDir};
use regex::Regex;
use flate2::read::GzDecoder;
use toml::Value;
use bincode2::{deserialize_from, serialize_into};

// A custom Result type that accepts generic types and uses an Error trait to dynamically handle errors.
type MyResult<T> = Result<T, Box<dyn Error>>;

// Configuration struct for a manual page.
#[derive(Debug)]
pub struct Config {
    // (Currently working with a single page at a time. Needs to be updated to a Vector to handle a file queue that can load multiple pages sequentially.) 
    page: String,
    // (Needs to be updated to a String to be able to handle section suffixes such as "1ssl" etc..)
    section: u8,
    file_path: String,
}

// Implementation for new Configs.
impl Config {
    pub fn new() -> MyResult<Config> {
        // Set default values.
        let mut page = String::from("-");
        let mut section = 1;
        let mut default_path = default_file_path();
        let mut file_path = format!("{}/man{}/{}.{}.gz", default_path.trim_matches('"'), section, page, section);
        let source_dir = env::current_dir().unwrap();
        let index_bin_path = PathBuf::from(&source_dir).join("index.bin");
    
        // Check if a bin file exists for the index cache. If not then create one.
        if !index_bin_path.exists() {
            index_cache();
        }
    
        // Collect user arguments.
        let args: Vec<String> = env::args().collect();
    
        // Match user arguments according to the number supplied and subsequent details.
        match args.len() {
            // If no arguments provided ask which manual page wanted.
            1 => {
                println!("What manual page do you want?\nFor example, try 'manr manr'.");
            },
            // If one argument is provided treat it as the manual page name and provide the lowest related section number. 
            // Or else check if a section number or flag/option and if valid ask for additional argument.
            2 => {
                // Check if a section number between 1-9 and if so ask for a related manual page.
                if let Ok(section) = args[1].clone().parse::<u8>() {
                    if (1..=9).contains(&section) {
                        println!("No manual entry for {}\n(Alternatively, what manual page do you want from section {}?)\nFor example, try 'manr manr'.", section, section);
                    }
                // Check if command to update index cache. Or check if a valid flag/option and ask for related argument.
                } else if let Some(arg) = Some(args[1].clone()) {
                    match arg.as_str() {
                        // Command to update the index bin file containing all the manual page details. Runs automatically if empty.
                        // (Needs tweaked to check only for modified or added files since last run. Could also be auto run periodically using a cron job.)
                        "mandb" => {
                            index_cache();
                        },
                        flag if flag.starts_with("-f") || flag == "--whatis" => {
                            println!("whatis what?");
                        },
                        flag if flag.starts_with("-k") || flag == "--apropos" => {
                            println!("apropos what?")
                        },
                        // Check if argument begins with "--" or "-" and notify of unrecognised/invalid option. 
                        // Or if a valid manual page run the lowest available section number.
                        _ => {
                            if arg.starts_with("--") {
                                println!("manr: unrecognised option -- '{}'", arg);
                                help();
                            } else if arg.starts_with("-") {
                                println!("manr: invalid option -- '{}'", arg);
                                help();
                            } else {
                                first_section(arg);
                            }
                        },
                    }
                }
            },
            // If one section number or a command and one argument is provided.
            3 => {
                // Check if a section number between 1-9 and if so run related file path.
                if let Ok(section) = args[1].clone().parse::<u8>() {
                    if (1..=9).contains(&section) {
                        let page = args[2].clone().to_lowercase();
                        let file_path = format!("{}/man{}/{}.{}.gz", default_path.trim_matches('"'), section, page, section);
                        run(file_path);
                    } else {
                        // Run lowest section number available if valid manual name but provided section number is outside 1-9 range.
                        let page = args[2].clone().to_lowercase();
                        first_section(page);
                    }
                // Check if a flag/option is used and run the related function.
                } else if let Some(arg) = Some(args[1].clone()) {
                    match arg.as_str() {
                        flag if flag.starts_with("-f") || flag == "--whatis" => {
                            let page = args[2].clone().to_lowercase();
                            index_whatis_search(page);           
                        },
                        flag if flag.starts_with("-k") || flag == "--apropos" => {
                            let search_term = args[2].clone().to_lowercase();
                            index_apropos_search(search_term);           
                        },
                        // (Check if additional arguments are valid manual page names and if so add to a viewing queue (unimplemented!)).
                        // Or if begins with "--" or "-" notify of unrecognised/invalid option.
                        _ => {
                            if arg.starts_with("--") {
                                println!("manr: unrecognised option -- '{}'", arg);
                                help();
                            } else if arg.starts_with("-") {
                                println!("manr: invalid option -- '{}'", arg);
                                help();
                            } else {
                                println!("No manual entry for {}. Please provide only one manual name at a time.", arg);
                                help();
                            }
                        },
                    }
                }
            },
            // For all the other cases show an error/help message. (In the future support a loading queue for multiple valid manual page arguments).
            _ => {
                println!("No manual entry. Please provide only one manual name at a time.");
                help();
            }
        }

        Ok(Config{
            section,
            page,
            file_path,
        })
    }
}

// Get default directory for manual pages from config.toml.
fn default_file_path() -> String {
    let mut config_toml = File::open("config.toml").expect("Problem opening config.toml");
    let mut config_str = String::new();
    config_toml.read_to_string(&mut config_str).expect("Problem reading config.toml to String");

    let config_file: Value = toml::from_str(&config_str).expect("Problem parsing values from config.toml");
    let default_path = config_file["default"]["file_path"].to_string();
    
    default_path
}

// Run and display manual files.
fn run(path: String) -> MyResult<()> {
    // Extract gzip manual file.
    let mut contents = extract_gzip(path);

    // Load extracted gzip contents into groff application with UTF-8 formatting. (Seems to have issue formatting numbered/nested lists.)
    let mut groff = Command::new("groff")
    .arg("-mandoc")
    .arg("-Tutf8")
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .spawn()
    .unwrap();

    {
        let stdin = groff.stdin.as_mut().unwrap();
        stdin.write_all(contents.as_bytes()).unwrap();
    }

    groff.wait().unwrap();

    // Pass groff's formatted document into the less viewer application.
    let mut less = Command::new("less")
    .arg("--quit-if-one-screen")
    .arg("-R")
    .stdin(groff.stdout.unwrap())
    .stdout(Stdio::inherit())
    .spawn()
    .expect("Failed to spawn less process");

    less.wait().unwrap();

    Ok(())
}

// Extract gzip files into String contents.
fn extract_gzip(path: String) -> String {
    // Open the file and read it into a buffer. If there's an error match it to the kind of error.
    let mut file = match File::open(path.clone()) {
        Ok(file) => file,
        Err(e) => {
            match e.kind() {
                // (Need to turn path into a page name for better error messages. Either using a function or Path file type parameter instead of String.)
                ErrorKind::NotFound => println!("No manual entry for {}", &path),
                ErrorKind::PermissionDenied => println!("Permission denied for {}", &path),
                _ => println!("Error opening file {:?}", e),
            }
            process::exit(1);
        }
    };
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).unwrap();

    // Extract the contents of the buffer into a String.
    let mut gzip = GzDecoder::new(&buffer[..]);
    let mut contents = String::new();
    // Check if the file extracted successfully and if not print the error.
    match gzip.read_to_string(&mut contents) {
        Ok(extracted) => Ok(extracted),
        Err(e) =>
            Err(println!("Error extracting gzip file {:?}: {}", gzip, e)),
    };

    contents
}

// Recursively list and sort all sections within a configured search directory.
fn list_all_sections() -> Vec<DirEntry> {
    let default_path = default_file_path();
    let search_dir = default_path.trim_matches('"');
    // A regex for a suffix covering filenames formatted like "name.1.gz" with a numeric range of 1-9, as well as optional alphabetic characters before the .gz extension (ie: name.1ssl.gz).
    let suffix = Regex::new(r"\.([1-9])(?:[a-zA-Z]*)?\.gz$").unwrap();

    // List all files in a search directory adhering to the regex pattern.
    let mut files: Vec<DirEntry> = WalkDir::new(search_dir)
        .follow_links(true)
        .into_iter()
        .filter_map(|result| result.ok())
        .filter(|result| result.file_type().is_file())
        .filter(|result| suffix.is_match(result.file_name().to_string_lossy().as_ref()))
        .collect();
    
    // Sort a page's sections in a ascending order according to the numeric range of the suffix.
    files.sort_by_key(|entry| {
        let sort_sections = suffix.captures(entry.file_name().to_string_lossy().as_ref()).unwrap()[1].parse::<u32>().unwrap();
        sort_sections
    });

    files
}

// Format filename and short description for displaying in terminal (ie: name (1) - description text).
fn format_filename_description(filename: walkdir::DirEntry) -> String {
    let description = get_description(filename.path().to_str().unwrap().to_owned());
    let mut result = String::new();
        
    // Format filenames by removing .gz extension and splitting at last "." character. Then add relevant description.
    if let Some(mut title) = Some(filename.file_name().to_str().expect("Failed to extract title from filename")) {
        let mut title = title.trim_end_matches(".gz").rsplitn(2, '.');
        let suffix = title.next().unwrap();
        let page = title.next().unwrap();

        let new_filename = format!("{} ({}) {}", page, suffix, description);
        result.push_str(&new_filename);
    }

    result
}

// Find and run/display the lowest section number if only one argument provided by user.
fn first_section(page: String) -> MyResult<()> {
    let files: HashMap<u32, Cache> = deserialise_index();
    let mut results: Vec<String> = Vec::new();

    for (_, cache) in files.iter() {
        if cache.page == page {
            results.push(format!("{}", cache.file_path));
        }
    }

    // Sort different section numbers in ascending order.
    results.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));

    // Check at least one page section exists before trying to run first file path.
    if results.is_empty() {
        println!("No manual entry for {}", page);
    } else {
        let first_file = results[0].to_string();
        run(first_file);
    }

    Ok(())
}

// Search the contents and troff/markdown formatting of a file and get the description.
fn get_description(path: String) -> String {
    let mut description = String::new();
    let contents = extract_gzip(path);
    let mut lines: Vec<&str> = Vec::new();

    // Push each line of a file's contents into a vector.
    for line in contents.lines() {
        lines.push(line);
    }

    // Turn vector into a iterator.
    let mut iter = lines.iter();
    // An additional check to break out of a loop after a pattern is found once.
    let mut found: bool = false;

    // Iterate over a vector's lines while they exist or until they match a pattern.
    while let Some(line) = iter.next() {
        // If line contains the relevant troff/markdown formatting then get the description from the next lines.
        if line.to_lowercase().contains(".sh name") || line.to_lowercase().contains(".sh \"name\"") {
            while let Some(next_lines) = iter.next() {
                // Check if the next lines contain or end with additional formatting.
                if next_lines.to_lowercase().contains(".nd") {
                    // If line ends with additional formatting then skip it and get description from the following line.
                    if next_lines.ends_with(".nd") || next_lines.ends_with(".nd ") {
                        if let Some(following_line) = iter.next() {
                            let text = "- ".to_owned() + &following_line.to_lowercase();
                            description.push_str(&text);
                            found = true;
                            break;
                        }
                    } else {
                        let text = next_lines.to_lowercase().replacen(".nd ", "- ", 1);
                        description.push_str(&text);
                        found = true;
                        break;
                    }
                } else {
                    if next_lines.contains("- ") || next_lines.ends_with("-") {
                        if next_lines.ends_with("-")  || next_lines.ends_with("- ") || next_lines.ends_with("- \\") || next_lines.ends_with("- \\ ") {
                            if let Some(following_line) = iter.next() {
                                let text = "- ".to_owned() + &following_line.to_lowercase();
                                description.push_str(&text);
                                found = true;    
                                break; 
                            }                   
                        } else {
                            let text = "- ".to_owned() + &next_lines.to_lowercase().split("- ").last().unwrap().to_string();
                            description.push_str(&text);
                            found = true;
                            break;
                        }
                    }
                }
            }
        }
        if found {
            break;
        }
    }

    description
}

// An index cache struct for entry values to be stored in a related HashMap.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct Cache {
    id: u32,
    page: String,
    section: String,
    description: String,
    file_path: String,
}

// Create an index cache HashMap for faster searching of manual pages and short descriptions. Automatically runs if empty.
// Can be updated on demand by using the mandb command or could be auto run periodically using a cron job.
// (Needs modified to only update files changed or added since last run.)
fn index_cache() -> std::io::Result<()> {
    let mut index = HashMap::new();
    let all_files: Vec<DirEntry> = list_all_sections();
    let mut results: Vec<String> = Vec::<String>::new();
    // Initialise a counter for unique ids in the index HashMap.
    let mut counter = 0;

    // Populate a vector with entries containing all index details concatenated.
    for file in all_files {
        let filename = format_filename_description(file.clone());
        let file_path = file.path().to_str().unwrap();
        let result = filename + " " + file_path;
               
        results.push(result);
    }

    for entry in results {
        if !entry.is_empty() {
            // Increase count by one for each new HashMap entry.
            counter += 1;

            // Populate index cache struct with split values.
            let index_details = Cache {
                id: counter,
                page: entry.split_whitespace().nth(0).unwrap_or("").to_owned(),
                section: entry.split_whitespace().nth(1).map(|s| s.trim_matches(|c| c == '(' || c == ')')).unwrap_or("").to_owned(),
                description: entry.split_once(" /").unwrap().0.split(" - ").last().unwrap_or("").to_owned(),
                file_path: entry.split_whitespace().last().unwrap_or("").to_owned(),
            };

            // Insert index struct values into a HashMap.
            index.insert(counter.clone(), index_details);
        }
    }

    // Serialise the index cache into a bin file.
    let bin_file = File::create("index.bin").unwrap();
    let mut buffer = BufWriter::new(bin_file);
    match bincode2::serialize_into(&mut buffer, &index) {
        Ok(_) => Ok(()),
        Err(e) => Err(std::io::Error::new(std::io::ErrorKind::Other, e)),
    };
       
    Ok(())
}

// Deserialise the index bin file.
fn deserialise_index() -> HashMap<u32, Cache> {
    let file = File::open("index.bin").unwrap();
    let buffer = BufReader::new(file);
    let index: HashMap<u32, Cache> = bincode2::deserialize_from(buffer).unwrap();

    index
}

// Search the index filenames for exact whatis matches.
fn index_whatis_search(search_term: String) -> MyResult<()> {
    let index: HashMap<u32, Cache> = deserialise_index();
    let mut results: Vec<String> = Vec::new();

    for (_, cache) in index.iter() {
        if cache.page == search_term {
            results.push(format!("{} ({}) - {}", cache.page, cache.section, cache.description));
        }
    }

    display_index_results(results, search_term);

    Ok(())
}

// Apropos search index filenames and short descriptions for results containing a search term.
fn index_apropos_search(search_term: String) -> MyResult<()> {
    let index: HashMap<u32, Cache> = deserialise_index();
    let mut results: Vec<String> = Vec::new();

    for (_, cache) in index.iter() {
        if cache.page.contains(&search_term) || cache.description.contains(&search_term) {
            results.push(format!("{} ({}) - {}", cache.page, cache.section, cache.description));
        }
    }

    display_index_results(results, search_term);

    Ok(())
}

// Sort and display index search results.
fn display_index_results(mut results: Vec<String>, search_term: String) -> MyResult<()> {
    // Sort different page names/section numbers in ascending order.
    results.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
    // Remove duplicate consecutive results from the sorted vector.
    results.dedup();

    if results.is_empty() {
        println!("{}: nothing appropriate", search_term);
    } else {
        for result in results {
            println!("{}", result);
        }
    }

    Ok(())
}

// A default help message to be displayed.
fn help() {
    println!("Try 'manr --help' or 'manr --usage' for more information.");
}