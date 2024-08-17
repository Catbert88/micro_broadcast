all: build_client build_server

build_client:
	cd client && cargo build --release -p client --target riscv32imc-unknown-none-elf

build_server:
	cd server && cargo build --release -p server

