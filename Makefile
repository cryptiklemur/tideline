.PHONY: dev

# On any rust change in crates/, plugins/, or src-tauri/src/, kill the running
# tauri dev tree, rebuild plugins, and restart tauri. Frontend hot-reload via
# vite still works untouched (we don't watch src/).
PLUGIN_PKGS := -p tideline-ptt -p tideline-notifications -p tideline-tones -p tideline-effects
HOST_PKG := -p tideline

# We rebuild the host (-p tideline) explicitly inside the watch command so the
# binary on disk is fresh BEFORE tauri-dev relaunches and execs it. Without
# this, tauri-dev would re-spawn the previous (stale) binary because cargo's
# incremental build hasn't completed yet at exec time.
dev:
	@cargo watch -c \
		-w crates -w plugins -w src-tauri/src \
		-d 0.5 \
		--why \
		-s 'cargo build $(PLUGIN_PKGS) $(HOST_PKG) && npm run tauri -- dev'
