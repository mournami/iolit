//go:build darwin

// Desktop integration changes are maintained separately from the upstream engine.
package main

import (
	"errors"
	"fmt"
	"net"
	"os"
	"path/filepath"
	"regexp"
	"strings"
)

func (d *Daemon) desktopEnabled() bool {
	b, err := os.ReadFile(filepath.Join(d.dirs.state, "desktop-enabled"))
	return err == nil && strings.TrimSpace(string(b)) == "1"
}

func (d *Daemon) saveDesktopEnabled(enabled bool) error {
	if !d.opts.desktop {
		return nil
	}
	value := []byte("0\n")
	if enabled {
		value = []byte("1\n")
	}
	return writeFileAtomic(filepath.Join(d.dirs.state, "desktop-enabled"), value)
}

var domainLabel = regexp.MustCompile(`^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$`)

func validateDesktopDomain(domain string) (string, error) {
	domain = strings.ToLower(strings.TrimSuffix(strings.TrimSpace(domain), "."))
	if len(domain) > 253 || !strings.Contains(domain, ".") || net.ParseIP(domain) != nil {
		return "", errors.New("a DNS domain is required")
	}
	for _, label := range strings.Split(domain, ".") {
		if !domainLabel.MatchString(label) {
			return "", errors.New("invalid domain label")
		}
	}
	return domain, nil
}

func (d *Daemon) AddDomain(input string) error {
	domain, err := validateDesktopDomain(input)
	if err != nil {
		return err
	}
	// Never accept a caller-supplied filename; only this installed user list is writable.
	path := filepath.Join(d.dirs.lists, "list-general-user.txt")
	b, err := os.ReadFile(path)
	if err != nil && !os.IsNotExist(err) {
		return err
	}
	if len(b) > 1024*1024 {
		return fmt.Errorf("user host list exceeds 1 MiB")
	}
	text := string(b)
	for _, line := range strings.Split(text, "\n") {
		if strings.TrimSpace(line) == domain {
			return nil
		}
	}
	if text != "" && !strings.HasSuffix(text, "\n") {
		text += "\n"
	}
	if err := writeFileAtomic(path, []byte(text+domain+"\n")); err != nil {
		return err
	}
	if d.isRunning() {
		return d.Reload()
	}
	return nil
}
