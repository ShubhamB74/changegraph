# Convenience targets; every one is a thin wrapper over a documented command.
.PHONY: check fmt clippy test frontend docs eval-quick demo sample

check:            ## everything CI runs
	scripts/check-all.sh

fmt:
	cargo fmt --all

clippy:
	cargo clippy --workspace --all-targets -- -D warnings

test:
	cargo test --workspace

frontend:
	cd frontend && npm install --no-audit --no-fund && npm run typecheck && npm run lint && npm test && npm run build

docs:
	node scripts/check-doc-links.mjs

eval-quick:       ## accuracy experiments (also run by `make test`)
	cargo run --release -q -p changegraph-eval -- scenarios
	cargo run --release -q -p changegraph-eval -- synthetic --sizes 100,1000

demo:
	scripts/demo.sh

sample:           ## regenerate the viewer's bundled sample from real analyzer output
	UPDATE_GOLDEN=1 cargo test -p changegraph-core --test golden
