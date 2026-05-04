.PHONY: dev

dev:
	@echo "==> initial plugin build"
	@cargo build -p tideline-ptt -p tideline-notifications -p tideline-tones -p tideline-wave-xlr -p tideline-effects
	@echo "==> starting cargo watch (plugins) + tauri dev"
	@trap 'kill 0' EXIT INT TERM; \
	 cargo watch -c -w plugins -x 'build -p tideline-ptt -p tideline-notifications -p tideline-tones -p tideline-wave-xlr -p tideline-effects' & \
	 npm run tauri -- dev; \
	 wait
