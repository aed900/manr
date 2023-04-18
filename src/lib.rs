use std::{env, fs::File, error::Error, path::PathBuf, process, process::Command, process::Stdio, 
    io, io::prelude::*, io::Write, io::BufReader, io::BufWriter, io::ErrorKind, collections::HashMap};
use walkdir::{DirEntry, WalkDir};
use regex::Regex;
use flate2::read::GzDecoder;
use toml::Value;
use log::error;

// A custom Result type that accepts generic types and uses Error trait to dynamically handle errors.
type BoxResult<T> = Result<T, Box<dyn Error>>;

// Set whether a function fails on errors or simply logs them.
#[derive(PartialEq)]
pub enum ErrorAction {
    Fail,
    Log,
}

// Get and parse user arguments and take appropriate actions.
pub fn get_args() -> BoxResult<()> {
    // Set default values.
    let default_path = default_file_path()?.to_string();
    let source_dir = env::current_dir()?;
    let index_bin_path = PathBuf::from(&source_dir).join("index.bin");
    
    // Check if a bin file exists for the index cache. If not then create one.
    if !index_bin_path.exists() {
        index_cache()?;
    }
    
    // Collect user arguments.
    let args: Vec<String> = env::args().collect();
    
    // Match user arguments according to the number supplied and subsequent details.
    match args.len() {
        // If no arguments provided ask which manual page wanted.
        1 => {
            println!("What manual page do you want?\nFor example, try 'manr man'.");
        },
        // If one argument is provided treat it as the manual page name and provide the lowest related section number. 
        // Or else check if a section number or flag/option and if valid ask for additional argument.
        2 => {
            // Check if a section number between 1-9 and if so ask for a related manual page.
            if let Ok(section) = args[1].clone().parse::<u8>() {
                if (1..=9).contains(&section) {
                    println!("No manual entry for {}\n(Alternatively, what manual page do you want from section {}?)\nFor example, try 'manr man'.", section, section);
                }
            // Else check if command to update index cache or a valid flag/option and if the latter ask for related argument.
            } else if let Some(arg) = Some(args[1].clone()) {
                match arg.as_str() {
                    // Command to update the index bin file containing all the manual page details. Runs automatically if empty.
                    // (Needs tweaked to check only for modified or added files since last run. Could also be auto run periodically using a cron job.)
                    "makewhatis" => {
                        index_cache()?;
                    },
                    flag if flag.starts_with("-f") || flag == "--whatis" => {
                        println!("whatis what?");
                    },
                    flag if flag.starts_with("-k") || flag == "--apropos" => {
                        println!("apropos what?")
                    },
                    // Check if argument begins with "--" or "-" and notify of unrecognised/invalid option. 
                    // Or else check if a valid manual page by running the lowest available section number.
                    _ => {
                        if arg.starts_with("--") {
                            println!("manr: unrecognised option -- '{}'", arg);
                            help();
                        } else if arg.starts_with("-") {
                            println!("manr: invalid option -- '{}'", arg);
                            help();
                        } else {
                            first_section(arg)?;
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
                    let file_path = format!("{}/man{}/{}.{}.gz", default_path, section, page, section);
                    run(file_path)?;
                } else {
                    // Else run lowest section number available if valid manual name but provided section number is outside 1-9 range.
                    let page = args[2].clone().to_lowercase();
                    first_section(page)?;
                }
            // Check if a flag/option is used and run the related function.
            } else if let Some(arg) = Some(args[1].clone()) {
                match arg.as_str() {
                    flag if flag.starts_with("-f") || flag == "--whatis" => {
                        let page = args[2].clone().to_lowercase();
                        index_whatis_search(page)?;           
                    },
                    flag if flag.starts_with("-k") || flag == "--apropos" => {
                        let search_term = args[2].clone().to_lowercase();
                        index_apropos_search(search_term)?;           
                    },
                    // Check if a section number, including those with an extended suffix including text, such as "1ssl".
                    sect if sect.chars().next().unwrap().is_digit(10) => {
                        let section = &arg;
                        let sect_num = sect.chars().next().unwrap().to_string();
                        let page = args[2].clone().to_lowercase();
                        let file_path = format!("{}/man{}/{}.{}.gz", default_path, sect_num, page, section);
                        run(file_path)?;
                    },
                    // Check if additional arguments are valid manual page names and if so open sequentially.
                    // (Needs a file queue to prompt user to continue, skip or quit between each file.)
                    // Or if begins with "--" or "-" notify of unrecognised/invalid option.
                    _ => {
                        if arg.starts_with("--") {
                            println!("manr: unrecognised option -- '{}'", arg);
                            help();
                        } else if arg.starts_with("-") {
                            println!("manr: invalid option -- '{}'", arg);
                            help();
                        } else {
                            let page1 = arg.to_lowercase();
                            let page2 = args[2].clone().to_lowercase();
                            first_section(page1)?;
                            first_section(page2)?;
                        }
                    },
                }
            }
        },
        // For all the other cases check if a section or manual page is provided and load multiple files sequentially. 
        // (Also needs to use file queue when implemented.)
        _ => {
            // Iterate over collected user arguments and skip the first default.
            let mut args_iter = args.iter().skip(1);
            // While arguments exist loop through them.
            while let Some(arg) = args_iter.next().clone() {
                match arg.as_str() {
                    // Check if a section number, optionally with an extended text suffix (such as "1ssl").
                    sect if sect.chars().next().unwrap().is_digit(10) => {
                        let section = &arg.to_lowercase();
                        let sect_num = sect.chars().next().unwrap().to_string().to_lowercase();
                        let page = args_iter.next().clone().unwrap().to_string().to_lowercase();
                        let file_path = format!("{}/man{}/{}.{}.gz", default_path, sect_num, page, section);
                        run(file_path)?;
                    }
                _ => {
                    // Otherwise treat argument as a manual page name without a section specified.
                    let page = arg.to_string().to_lowercase();
                    first_section(page)?;
                    }
                }
            }
        }
    }
    
    Ok(())
}

// Get default directory for manual pages from config.toml.
fn default_file_path() -> BoxResult<String> {
    // Load the config file contents into a new String.
    let mut config_toml = File::open("config.toml")?;
    let mut config_str = String::new();
    config_toml.read_to_string(&mut config_str)?;

    // Parse the values from the config file.
    let config_file: Value = toml::from_str(&config_str)?;
    let default_path = config_file["default"]["file_path"].to_string();

    Ok(default_path.trim_matches('"').to_string())
}

// Run and display manual files.
pub fn run(path: String) -> BoxResult<()> {
    // Extract gzip manual file and set action on errors to fail.
    let contents = extract_gzip(path, ErrorAction::Fail)?.to_string();

    // Load extracted gzip contents into groff application with UTF-8 formatting. (Seems to have issue formatting numbered/nested lists.)
    let mut groff = Command::new("groff")
    .arg("-mandoc")
    .arg("-Tutf8")
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .spawn()?;

    {
        let stdin = groff.stdin.as_mut().unwrap();
        stdin.write_all(contents.as_bytes())?;
    }

    groff.wait()?;

    // Pass groff's formatted document into the less viewer application.
    let mut less = Command::new("less")
    .arg("-R")
    .stdin(groff.stdout.unwrap())
    .stdout(Stdio::inherit())
    .spawn()?;

    less.wait()?;

    Ok(())
}

// Open a file and read its contents into a Vector.
fn open_file(path: String) -> BoxResult<Vec<u8>> {
    let mut file = File::open(path.clone())?;
    let mut contents = Vec::new();
    file.read_to_end(&mut contents)?;
    Ok(contents)
}

// Extract gzip files into String contents.
pub fn extract_gzip(path: String, errors: ErrorAction) -> BoxResult<String> {
    // Split file path from filename and format name by removing .gz extension and splitting at last "." character. 
    let file_path = path.clone();
    let mut filename = file_path.split("/").last().unwrap().trim_end_matches(".gz").rsplitn(2, '.');
    let section = filename.next().unwrap();
    let page = filename.next().unwrap();

    // Open the file path and read its contents into a new Vector. 
    let file_result = open_file(path.clone());
    let mut contents = Vec::new();

    // Match any errors to their kind and either print/exit or log/continue depending on setting of ErrorAction.
    if errors == ErrorAction::Fail {
        match file_result {
            Ok(file) => {
                contents = file;
            },
            Err(e) => {
                // Downcast boxed error to type that implements the std Error trait.
                if let Some(err) = e.downcast_ref::<io::Error>() {
                    match err.kind() {
                        ErrorKind::NotFound => println!("No manual entry for {} in section {}", &page, &section),
                        ErrorKind::PermissionDenied => println!("Permission denied for {} in section {}", &page, &section),
                        _ => println!("Error opening file {:?}", err),
                    }
                }
            process::exit(1);
            }
        };
    } else {
        match file_result {
            Ok(file) => {
                contents = file;
            },
            Err(e) => {
                if let Some(err) = e.downcast_ref::<io::Error>() {
                    match err.kind() {
                        ErrorKind::NotFound => error!("No manual entry for {} in section {}", &page, &section),
                        ErrorKind::PermissionDenied => error!("Permission denied for {} in section {}", &page, &section),
                        _ => error!("Error opening file {:?}", err),
                    }
                }
            }
        };
    }

    // Extract the contents of the opened file into a String.
    let mut gzip = GzDecoder::new(&contents[..]);
    let mut gzip_contents = String::new();
    // Check if the file extracted successfully and if not log the error and continue.
    match gzip.read_to_string(&mut gzip_contents) {
        Ok(extracted) => Ok(extracted),
        Err(e) =>
            Err(error!("Error extracting gzip file for {} in section {}: {}", page, section, e)),
    };

    Ok(gzip_contents)
}

// Recursively list and sort all sections within a configured search directory.
fn list_all_sections() -> BoxResult<Vec<DirEntry>> {
    let default_path = default_file_path()?.to_string();

    // A regex for a suffix covering filenames formatted like "name.1.gz" or "name.1ssl.gz" with a numeric range of 1-9.
    let suffix = Regex::new(r"\.([1-9])(?:[a-zA-Z]*)?\.gz$")?;

    // List all files in a search directory adhering to the regex pattern.
    let mut files: Vec<DirEntry> = WalkDir::new(default_path)
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

    Ok(files)
}

// Format filename and short description for displaying in terminal (ie: name (1) - description text).
fn format_filename_description(path: String) -> BoxResult<String> {
    let description = get_description(path.clone())?.to_string();
    let mut result = String::new();
        
    // Split path from filename and format filenames by removing .gz extension and splitting at last "." character. Then add relevant description.
    if let Some(filename) = Some(path.split("/").last().unwrap()) {
        let mut title = filename.trim_end_matches(".gz").rsplitn(2, '.');
        let section = title.next().unwrap();
        let page = title.next().unwrap();

        let new_filename = format!("{} ({}) - {}", page, section, description);
        result.push_str(&new_filename);
    }

    Ok(result)
}

// Find and run/display the lowest section number if none is provided by user.
fn first_section(page: String) -> BoxResult<()> {
    // Load all entries in the index cache and create a new results Vector.
    let files: HashMap<u32, Cache> = deserialise_index()?;
    let mut results: Vec<String> = Vec::new();

    // Match page arg with page in the index cache and pass its file path to the Vector.
    for (_, cache) in files.iter() {
        if cache.page == page {
            results.push(format!("{}", cache.file_path));
        }
    }

    // Sort different section numbers in ascending order.
    results.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));

    // Check at least one page section exists before trying to run the first file path.
    if results.is_empty() {
        println!("No manual entry for {}", page);
    } else {
        let first_file = results[0].to_string();
        run(first_file)?;
    }

    Ok(())
}

// Search the contents and troff/markdown formatting of a file and get the description.
fn get_description(path: String) -> BoxResult<String> {
    let mut description = String::new();
    let contents = extract_gzip(path, ErrorAction::Log)?.to_string();
    let mut lines: Vec<&str> = Vec::new();

    // Push each line of a file's contents into a Vector.
    for line in contents.lines() {
        lines.push(line);
    }

    // Turn the Vector into a iterator.
    let mut iter = lines.iter();
    // An additional check to break out of a loop after a pattern is found once.
    let mut found: bool = false;

    // Iterate over the Vector's lines while they exist or until they match a pattern.
    while let Some(line) = iter.next() {
        // If line contains the relevant troff/markdown formatting then get the description from the next lines.
        if line.to_lowercase().contains(".sh name") || line.to_lowercase().contains(".sh \"name\"") {
            while let Some(next_lines) = iter.next() {
                // Check if the next lines contain or end with additional formatting cointaining .nd.
                if next_lines.to_lowercase().contains(".nd") {
                    // If the trimmed line ends with additional formatting then skip it and get description from the following line.
                    if next_lines.trim_end().ends_with(".nd") {
                        if let Some(following_line) = iter.next() {
                            let text = &following_line.to_lowercase();
                            description.push_str(&text);
                            found = true;
                            break;
                        }
                    // Else if the next lines don't end with .nd remove the ".nd " formatting and get the description from that line.
                    } else {
                        let text = next_lines.to_lowercase().replacen(".nd ", "", 1).replacen(".nd", "", 1);
                        description.push_str(&text);
                        found = true;
                        break;
                    }
                // Else check if the next lines contain "-" and after trimming, if ends with additional "-" or "- \\" formatting. 
                // Then get description.
                } else {
                    if next_lines.contains("-") {
                        if next_lines.trim_end().ends_with("-")  || next_lines.trim_end().ends_with("- \\") {
                            if let Some(following_line) = iter.next() {
                                let text = &following_line.to_lowercase();
                                description.push_str(&text);
                                found = true;    
                                break; 
                            }
                        // Else if next lines don't end with "-" then split on the next line if ut has "- " formatting to get description.              
                        } else {
                            if let Some(text) = Some(&next_lines.to_lowercase().split("- ").last().unwrap().to_string()) {
                                description.push_str(&text);
                                found = true;
                                break;
                            }

                        }
                    }
                }
            }
        }
        if found {
            break;
        }
    }

    Ok(description)
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
// Can be updated on demand by using the makewhatis command or could be auto run periodically using a cron job.
// (Needs modified to only update files changed or added since last run.)
fn index_cache() -> BoxResult<std::io::Result<()>> {
    let mut index = HashMap::new();
    let all_files: Vec<DirEntry> = list_all_sections()?;
    let mut results: Vec<String> = Vec::<String>::new();
    // Initialise a counter for unique ids in the index HashMap.
    let mut counter = 0;

    // Populate a Vector with entries containing all index details concatenated.
    for file in all_files {
        let filename = format_filename_description(file.clone().path().to_str().unwrap().to_owned())?.to_string();
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
                page: entry.split_whitespace().nth(0).unwrap_or("#").to_owned(),
                section: entry.split_whitespace().nth(1).map(|s| s.trim_matches(|c| c == '(' || c == ')')).unwrap_or("").to_owned(),
                description: entry.split_once(" /").unwrap().0.split(" - ").last().unwrap_or("").to_owned(),
                file_path: entry.split_whitespace().last().unwrap_or("").to_owned(),
            };

            // Insert index struct values into a HashMap.
            index.insert(counter.clone(), index_details);
        }
    }

    // Serialise the index cache into a bin file.
    let bin_file = File::create("index.bin")?;
    let mut buffer = BufWriter::new(bin_file);
    match bincode2::serialize_into(&mut buffer, &index) {
        Ok(_) => Ok(()),
        Err(e) => Err(std::io::Error::new(std::io::ErrorKind::Other, e)),
    }?;

    // Notify user that database was successfully updated.
    println!("Successfully updated manual entries in database.");
       
    Ok(Ok(()))
}

// Deserialise the index bin file.
fn deserialise_index() -> BoxResult<HashMap<u32, Cache>> {
    let file = File::open("index.bin")?;
    let buffer = BufReader::new(file);
    let index: HashMap<u32, Cache> = bincode2::deserialize_from(buffer)?;

    Ok(index)
}

// Search the index filenames for exact whatis matches.
fn index_whatis_search(search_term: String) -> BoxResult<()> {
    let index: HashMap<u32, Cache> = deserialise_index()?;
    let mut results: Vec<String> = Vec::new();

    for (_, cache) in index.iter() {
        if cache.page == search_term {
            results.push(format!("{} ({}) - {}", cache.page, cache.section, cache.description));
        }
    }

    display_index_results(results, search_term)?;

    Ok(())
}

// Apropos search index filenames and short descriptions for results containing a search term.
fn index_apropos_search(search_term: String) -> BoxResult<()> {
    let index: HashMap<u32, Cache> = deserialise_index()?;
    let mut results: Vec<String> = Vec::new();

    for (_, cache) in index.iter() {
        if cache.page.contains(&search_term) || cache.description.contains(&search_term) {
            results.push(format!("{} ({}) - {}", cache.page, cache.section, cache.description));
        }
    }

    display_index_results(results, search_term)?;

    Ok(())
}

// Sort and display index search results.
fn display_index_results(mut results: Vec<String>, search_term: String) -> BoxResult<()> {
    // Sort different page names/section numbers in ascending order.
    results.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
    // Remove duplicate consecutive results from the sorted Vector.
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
