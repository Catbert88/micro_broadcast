# Helpful to export flash port:
# export ESPFLASH_PORT="/dev/cu.usbmodem1101"

build: build_client build_server

build_client:
	cd client && cargo build --release -p client 

build_server:
	cd server && cargo build --release -p server

run_client:
	cd client && cargo run --release -p client 
