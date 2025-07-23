all : vchat vchatd
vchat : vanillachat/src/main.rs
	cd vanillachat; cargo build --release; mv target/release/vanillachat ../$@
vchatd : vanillachatd/src/main.rs
	cd vanillachatd; cargo build --release; mv target/release/vanillachatd ../$@
install : all
	install vchat /usr/bin
	install vchatd /usr/bin
