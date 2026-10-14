.PHONY: dev run start once serve web post-one build check test stats judge-reset stop port

dev: stop
	cargo watch -w src -w config.toml -x 'run -- --web-only'

run: stop
	op run --env-file=.env.op -- cargo watch -w src -w config.toml -x run

start: stop
	op run --env-file=.env.op -- cargo run

once: stop
	op run --env-file=.env.op -- cargo run -- --once

serve: stop
	op run --env-file=.env.op -- cargo run -- --no-post

web: stop
	cargo run -- --web-only

post-one:
	op run --env-file=.env.op -- cargo run -- --post-one

build:
	cargo build

check:
	cargo clippy --all-targets && cargo fmt --check

stop:
	@pkill -f 'target/debug/bluesky-techbot' 2>/dev/null; true

port:
	@lsof -nP -iTCP:8080 -sTCP:LISTEN || echo "8080番は空いています"

stats:
	@sqlite3 techbot.db "SELECT source, COUNT(*) FROM articles GROUP BY source;"
	@sqlite3 techbot.db "SELECT COALESCE(verdict, CASE WHEN fetch_error IS NOT NULL THEN 'fetch-failed' ELSE 'unjudged' END) AS state, COUNT(*) FROM articles GROUP BY state;"

judge-reset:
	sqlite3 techbot.db "UPDATE articles SET judged_at=NULL, judge_attempts=0, verdict=NULL, reason=NULL, field=NULL, industry=NULL, form=NULL, basis=NULL, depth=NULL, judge_error=NULL, post_title=NULL, post_body=NULL, post_point=NULL, status=CASE WHEN posted_at IS NOT NULL THEN 'posted' ELSE 'collected' END;"

test:
	cargo test
