# manr
A prototype Rust version of man - "an interface to the system reference manuals"

This program currently requires groff and less to also be installed. 
Depending on Linux distro these can be installed if not already as follows:
sudo apt-get install groff
sudo apt-get install less

Open manual pages by running the program along with the page name or a section number and page name.
Examples:
To open the first available section:
cargo run man
To open a specific section:
cargo run 7 man

Currently supports using the -f flag with a search term for a whatis type search (lists sections) or the -k flag with a search term for an apropos type search (listing pages or descriptions containing a search term).

Examples: 
To list available sections for a page:
cargo run -- -f man

To find all pages and descriptions containing a search term:
cargo run -- -k man

An index.bin file is created if not found from all manual page entries recursively found in the default directory. The default directory is set to "/usr/share/man/" and can be changed in the config.toml file.

To update the index.bin when files are changed or added within this directory run the mandb command.
Example:
cargo run mandb

Alternatively delete any existing index.bin or setup a cron job to periodically refresh this file.
