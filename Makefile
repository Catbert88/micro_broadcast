# Helpful to export flash port:
# export ESPFLASH_PORT="/dev/cu.usbmodem1101"

build: build_client build_server

run: run_client run_server

build_client:
	cd client && cargo build --release -p client 

build_server:
	cd server && cross build --release -p server --target arm-unknown-linux-gnueabihf

run_client:
	cd client && cargo run --release -p client 

run_server: build_server
	rsync -t -r -avz -e "ssh -o StrictHostKeyChecking=no" target/arm-unknown-linux-gnueabihf/release/server  pi@192.168.4.209:/home/pi/tmp/mb_server.rs
