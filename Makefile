# PREFIX is environment variable, but if it is not set, then set default value
ifeq ($(PREFIX),)
	PREFIX := /usr/local
endif

.PHONY: install
install: mod-manager
	install -Dm755 mod-manager "$(DESTDIR)/$(PREFIX)/bin/mod-manager"
	install -Dm755 mod-manager-overlayfs-helper "$(DESTDIR)/$(PREFIX)/bin/mod-manager-overlayfs-helper"
	install -Dm644 mod-manager.policy "$(DESTDIR)/$(PREFIX)/share/polkit-1/actions/mod-manager.policy"
	install -Dm644 mod-manager.service "$(DESTDIR)/$(PREFIX)/lib/systemd/user/mod-manager.service"
