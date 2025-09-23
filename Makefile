# Development workflow for devtools

.PHONY: dev watch install test test-checkout test-squash test-rovo coverage coverage-console coverage-open install-coverage clean

# Fast development build and install
dev:
	cargo build && cp target/debug/dt ~/.local/bin/dt

# Watch mode for continuous development  
watch:
	cargo watch -x build -s "cp target/debug/dt ~/.local/bin/dt"

# Install release version
install:
	cargo build --release && cp target/release/dt ~/.local/bin/dt

# Run all tests
test:
	cargo test

# Quick test of checkout command
test-checkout:
	./target/debug/dt checkout --help
	./target/debug/dt checkout test-branch --dry-run

# Quick test of squash command  
test-squash:
	./target/debug/dt squash --help
	./target/debug/dt squash 3 --dry-run

# Quick test of rovo command
test-rovo:
	./target/debug/dt rovo --help

# Generate coverage report (requires cargo-tarpaulin)
coverage:
	@if ! command -v cargo-tarpaulin >/dev/null 2>&1; then \
		echo "❌ cargo-tarpaulin not found. Installing..."; \
		cargo install cargo-tarpaulin; \
	fi
	cargo tarpaulin --out html --output-dir coverage/
	@echo "✅ Coverage report generated!"
	@echo "📂 Open in browser: file://$(PWD)/coverage/tarpaulin-report.html"
	@echo "💡 Or run: open coverage/tarpaulin-report.html"

# Console coverage report
coverage-console:
	@if ! command -v cargo-tarpaulin >/dev/null 2>&1; then \
		echo "❌ cargo-tarpaulin not found. Installing..."; \
		cargo install cargo-tarpaulin; \
	fi
	cargo tarpaulin --out Stdout

# Install coverage tools
install-coverage:
	cargo install cargo-tarpaulin

# Generate coverage and open in browser
coverage-open:
	@if ! command -v cargo-tarpaulin >/dev/null 2>&1; then \
		echo "❌ cargo-tarpaulin not found. Installing..."; \
		cargo install cargo-tarpaulin; \
	fi
	cargo tarpaulin --out html --output-dir coverage/
	@echo "✅ Coverage report generated and opening in browser..."
	@open coverage/tarpaulin-report.html 2>/dev/null || xdg-open coverage/tarpaulin-report.html 2>/dev/null || echo "📂 Manually open: file://$(PWD)/coverage/tarpaulin-report.html"

# Clean build artifacts
clean:
	cargo clean

# Build and run quick functionality tests
test-all: dev test-checkout test-squash test-rovo
	@echo "✅ All quick tests completed"

# Check compilation without building
check:
	cargo check