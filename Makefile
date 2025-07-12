vanillachat : src/main.rs
	cargo build --release ; mv target/release/$@ .
install : vanillachat
	cp vanillachat /usr/bin
