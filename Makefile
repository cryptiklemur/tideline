.PHONY: dev

dev:
	@echo "building plugins (initial)..."
	cargo build -p tideline-ptt -p tideline-notifications -p tideline-tones -p tideline-wave-xlr -p tideline-effects
	@echo "starting tauri dev with cargo watch..."
	npm run tauri -- dev
