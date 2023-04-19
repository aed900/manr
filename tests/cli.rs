use assert_cmd::Command as AssertCommand;
use predicates::prelude::*;
use std::{io::prelude::*, io::BufReader, io::Read, error::Error, process::Command as StdCommand, process::Stdio};
use manr::*;

type TestResult = Result<(), Box<dyn Error>>;

// Constants for manual commands and page paths.
const PRG: &str = "manr";
const INDEX_CMD: &str = "makewhatis";
const PAGE_NOT_FOUND: &str = "manxcjgcj";
const SECT_NOT_FOUND: &str = "1gjopege";
const MAN1_GZ: &str = "./tests/inputs/man.1.gz";
const MAN7_GZ: &str = "./tests/inputs/man.7.gz";
const CAT1_GZ: &str = "./tests/inputs/cat.1.gz";
const CHMOD1_GZ: &str = "./tests/inputs/chmod.1.gz";
const CHMOD2_GZ: &str = "./tests/inputs/chmod.2.gz";
const CHROOT8_GZ: &str = "./tests/inputs/chroot.8.gz";
const PERM_DENIED_CMD: &str = "permdenied";
const PERM_DENIED_GZ: &str = "./tests/inputs/permdenied.1.gz";
const BAD_GZ_CMD: &str = "badgzip";
const BAD_GZ: &str = "./tests/inputs/badgzip.1.gz";

// A test function for the run function which normally extracts, formats and displays manual files.
// This function instead prints the stdout to a String. 
// Also possible to instead change the main function's return type to a Child to convert the stdout externally.
pub fn run_to_string(path: String) -> String {
    // Extract gzip manual file.
    let contents = extract_gzip(path, ErrorAction::Fail);

    // Load extracted gzip contents into groff application with UTF-8 formatting. (Seems to have issue formatting numbered/nested lists.)
    let mut groff = StdCommand::new("groff")
    .arg("-mandoc")
    .arg("-Tutf8")
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .spawn()
    .unwrap();

    {
        let stdin = groff.stdin.as_mut().unwrap();
        stdin.write_all(contents.expect("Couldn't write groff stdout").as_bytes()).unwrap();
    }

    groff.wait().unwrap();

    // Pass groff's formatted document into the less viewer application.
    let mut less = StdCommand::new("less")
    .arg("-R")
    .stdin(groff.stdout.unwrap())
    .stdout(Stdio::piped()) // Changed from inherit to piped for testing run_to_string purposes.
    .spawn()
    .expect("Failed to spawn less process");

    // Convert the less stdout to a String for testing purposes.
    let stdout = less.stdout.take().unwrap();
    let mut buffer = BufReader::new(stdout);
    let mut stdout_str = String::new();
    buffer.read_to_string(&mut stdout_str).unwrap();

    stdout_str
}

// Some test results may vary depending on what manuals are stored in default directory.

#[test]
fn run_and_extract_page_and_open_with_groff_and_less() -> TestResult {
    let page = "man";
    let expected = run_to_string(MAN1_GZ.to_string());

    AssertCommand::cargo_bin(PRG)?
        .args([&page])
        .assert()
        .stdout(predicate::str::contains(format!("{}", expected)));

    Ok(())
}

#[test]
fn run_and_extract_multiple_pages_and_open_sequentially_with_groff_and_less() -> TestResult {
    let page1 = "man";
    let page2 = "cat";
    let page3 = "chmod";
    let expected1 = run_to_string(MAN1_GZ.to_string());
    let expected2 = run_to_string(CAT1_GZ.to_string());
    let expected3 = run_to_string(CHMOD1_GZ.to_string());

    AssertCommand::cargo_bin(PRG)?
        .args([&page1, &page2, &page3])
        .assert()
        .stdout(predicate::str::contains(format!("{}", expected1)))
        .stdout(predicate::str::contains(format!("{}", expected2)))
        .stdout(predicate::str::contains(format!("{}", expected3)));

    Ok(())
}

#[test]
fn run_and_extract_page_with_section_and_open_with_groff_and_less() -> TestResult {
    let page = "man";
    let section = "7";
    let expected = run_to_string(MAN7_GZ.to_string());

    AssertCommand::cargo_bin(PRG)?
        .args([&section, &page])
        .assert()
        .stdout(predicate::str::contains(format!("{}", expected)));

    Ok(())
}

#[test]
fn run_and_extract_multiple_pages_with_sections_and_open__sequentially_with_groff_and_less() -> TestResult {
    let page1 = "man";
    let section1 = "7";
    let expected1 = run_to_string(MAN7_GZ.to_string());
    let page2 = "chmod";
    let section2 = "2";
    let expected2 = run_to_string(CHMOD2_GZ.to_string());
    let page3 = "chroot";
    let section3 = "8";
    let expected3 = run_to_string(CHROOT8_GZ.to_string());

    AssertCommand::cargo_bin(PRG)?
        .args([&section1, &page1, &section2, &page2, &section3, &page3])
        .assert()
        .stdout(predicate::str::contains(format!("{}", expected1)))
        .stdout(predicate::str::contains(format!("{}", expected2)))
        .stdout(predicate::str::contains(format!("{}", expected3)));

    Ok(())
}

#[test]
fn page_not_found() -> TestResult {
    let bad_page = PAGE_NOT_FOUND;
    let expected = format!("No manual entry for {}", bad_page);

    AssertCommand::cargo_bin(PRG)?
        .args([&bad_page])
        .assert()
        .stdout(predicate::str::is_match(expected)?);

    Ok(())
}

#[test]
fn page_not_found_when_opening_multiple() -> TestResult {
    let page1 = "man";
    let expected1 = run_to_string(MAN1_GZ.to_string());
    let page2 = "chmod";
    let expected2 = run_to_string(CHMOD1_GZ.to_string());
    let page3 = "cat";
    let expected3 = run_to_string(CAT1_GZ.to_string());
    let bad_page = PAGE_NOT_FOUND;
    let expected4 = format!("No manual entry for {}", bad_page);

    AssertCommand::cargo_bin(PRG)?
        .args([&page1, &page2, &page3, &bad_page])
        .assert()
        .stdout(predicate::str::contains(format!("{}", expected1)))
        .stdout(predicate::str::contains(format!("{}", expected2)))
        .stdout(predicate::str::contains(format!("{}", expected3)))
        .stdout(predicate::str::is_match(expected4)?);

    Ok(())
}

#[test]
fn page_section_not_found() -> TestResult {
    let page = "man";   // Same result for a bad page
    let bad_sect = SECT_NOT_FOUND;
    let expected = format!("No manual entry for {} in section {}\n", page, bad_sect);
    AssertCommand::cargo_bin(PRG)?
        .args([&bad_sect, &page])
        .assert()
        .stdout(predicate::str::is_match(expected)?);

    Ok(())
}

#[test]
fn page_section_not_found_when_opening_multiple() -> TestResult {
    let page1 = "man";
    let section1 = "7";
    let expected1 = run_to_string(MAN7_GZ.to_string());
    let page2 = "chmod";
    let section2 = "2";
    let expected2 = run_to_string(CHMOD2_GZ.to_string());
    let page3 = "chroot";
    let section3 = "8";
    let expected3 = run_to_string(CHROOT8_GZ.to_string());
    let page4 = "cat";    
    let bad_section = SECT_NOT_FOUND;
    let expected4 = format!("No manual entry for {} in section {}", page4, bad_section);

    AssertCommand::cargo_bin(PRG)?
        .args([&section1, &page1, &section2, &page2, &section3, &page3, &bad_section, &page4])
        .assert()
        .stdout(predicate::str::contains(format!("{}", expected1)))
        .stdout(predicate::str::contains(format!("{}", expected2)))
        .stdout(predicate::str::contains(format!("{}", expected3)))
        .stdout(predicate::str::is_match(expected4)?);

    Ok(())
}

#[test]
fn whatis_search() -> TestResult {
    let command = "-f";
    let page = "man";
    let expected = vec!["man (1) - an interface to the system reference manuals", "man (7) - macros to format man pages"];  
    AssertCommand::cargo_bin(PRG)?
        .args([&command, &page])
        .assert()
        .stdout(predicate::str::contains(expected.join("\n")));
  
    Ok(())
}

#[test]
fn whatis_search_not_found() -> TestResult {
    let command = "-f";
    let bad_page = PAGE_NOT_FOUND;
    let expected = format!("{}: nothing appropriate", bad_page);  
    AssertCommand::cargo_bin(PRG)?
        .args([&command, &bad_page])
        .assert()
        .stdout(predicate::str::is_match(expected)?);

    Ok(())
}

#[test]
fn apropos_search() -> TestResult {
    let command = "-k";
    let page = "zcat";
    let expected = vec!("bzcat (1) - a block-sorting file compressor, v1.0.8",
    "lzcat (1) - compress or decompress .xz and .lzma files",
    "xzcat (1) - compress or decompress .xz and .lzma files",
    "zcat (1) - compress or expand files");
    AssertCommand::cargo_bin(PRG)?
        .args([&command, &page])
        .assert()
        .stdout(predicate::str::contains(expected.join("\n")));

    Ok(())
}

#[test]
fn apropos_search_not_found() -> TestResult {
    let command = "-k";
    let bad_page = PAGE_NOT_FOUND;
    let expected = format!("{}: nothing appropriate", bad_page);  
    AssertCommand::cargo_bin(PRG)?
        .args([&command, &bad_page])
        .assert()
        .stdout(predicate::str::is_match(expected)?);

    Ok(())
}

// Requires a file with limited permissions in default search directory.
#[test]
fn index_cache_refresh() -> TestResult {
    let cmd = INDEX_CMD;
    let expected = format!("Successfully updated manual entries in database.");

    AssertCommand::cargo_bin(PRG)?
        .args([&cmd])
        .assert()
        .stdout(predicate::str::contains(expected));

    Ok(())
}

// Requires permdenied.1.gz or an alternative page with limited permissions in default search directory.
#[test]
fn page_open_permission_denied() -> TestResult {
    let bad_page = PERM_DENIED_CMD;
    let expected = format!("Permission denied for {} in section", bad_page);

    AssertCommand::cargo_bin(PRG)?
        .args([&bad_page])
        .assert()
        .stdout(predicate::str::contains(expected));

    Ok(())
}

// Requires badgzip.1.gz or an alternative page that can't be extracted as gzip in default search directory.
#[test]
fn gzip_extract_failed() -> TestResult {
    let bad_page = BAD_GZ_CMD;
    let expected = format!("Error extracting gzip file for {} in section", bad_page);

    AssertCommand::cargo_bin(PRG)?
        .args([&bad_page])
        .assert()
        .stderr(predicate::str::contains(expected));

    Ok(())
}

// (Need to implement:)
// (Tests for get descriptions from various .nd and "-" formatting)
// (Test for failed section without page)
