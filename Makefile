# Makefile for mod-manager

# Define installation directories
PREFIX ?= /usr/local
BINDIR = $(PREFIX)/bin
SHAREDIR = $(PREFIX)/share
POLKITDIR = $(SHAREDIR)/polkit-1/actions
SYSTEMDDIR = $(SHAREDIR)/systemd/user
ZSHDIR = $(SHAREDIR)/zsh/site-functions

# Define files to install
BIN_FILE = mod-manager
HELPER_FILE = mod-manager-overlayfs-helper
POLICY_FILE = mod-manager.policy
SERVICE_FILE = mod-manager.service
ZSH_FILE = _mod-manager

# Targets
.PHONY: build install test clean

build:
	@echo "Building mod-manager…"
	@cargo build --release

install:
	@echo "Installing files…"
	install -D -m 755 target/release/$(BIN_FILE) $(DESTDIR)$(BINDIR)/$(BIN_FILE)
	install -D -m 755 dist/$(HELPER_FILE) $(DESTDIR)$(BINDIR)/$(HELPER_FILE)
	install -D -m 644 dist/$(POLICY_FILE) $(DESTDIR)$(POLKITDIR)/$(POLICY_FILE)
	install -D -m 644 dist/$(SERVICE_FILE) $(DESTDIR)$(SYSTEMDDIR)/$(SERVICE_FILE)
	install -D -m 644 dist/$(ZSH_FILE) $(DESTDIR)$(ZSHDIR)/$(ZSH_FILE)

test:
	@echo "Testing…"
	@cargo test

clean:
	@echo "Cleaning up…"
	@cargo clean
