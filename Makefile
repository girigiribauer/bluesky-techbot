.PHONY: tunnel local-web local-once check stop

VPS ?= scotch

tunnel:
	@echo "つないでいる間だけ、http://localhost:8080 で本番の画面が見られます（抜けるには exit）"
	ssh $(VPS) -L 8080:127.0.0.1:8080

local-web: stop
	cargo watch -w src -w config.toml -x 'run -- --web-only'

local-once: stop
	op run --env-file=.env.op -- cargo run -- --once

check:
	cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test

stop:
	@pkill -f 'target/debug/bluesky-techbot' 2>/dev/null; true
