//go:build darwin

package main

import (
	"github.com/naladwepo/zapret-for-mac/internal/netcfg"
	"os"
	"path/filepath"
	"testing"
)

func TestDesktopDomainRejectsInjection(t *testing.T) {
	for _, s := range []string{"127.0.0.1", "https://example.com", "a..com", "example.com\n--fake", "*.com", "../foo", "single", "-a.example"} {
		if _, err := validateDesktopDomain(s); err == nil {
			t.Fatalf("accepted %q", s)
		}
	}
	if s, err := validateDesktopDomain("EXAMPLE.COM."); err != nil || s != "example.com" {
		t.Fatal(s, err)
	}
}

func TestDesktopStopDoesNotDependOnPreferenceWrite(t *testing.T) {
	state := t.TempDir()
	if err := os.Mkdir(filepath.Join(state, "desktop-enabled"), 0700); err != nil {
		t.Fatal(err)
	}
	d := &Daemon{opts: options{desktop: true}, dirs: dirs{state: state}, enabled: true}
	d.pf = netcfg.NewPF("desktop-stop-test", netcfg.PFOpts{StateDir: state, DryRun: true})
	defer d.pf.Close()
	if err := d.Stop(); err == nil {
		t.Fatal("preference error must be reported")
	}
	if d.isEnabled() {
		t.Fatal("failed preference write left interception enabled")
	}
}
func TestDesktopStartsDisabledAndPersistsStop(t *testing.T) {
	d := &Daemon{opts: options{desktop: true}, dirs: dirs{state: t.TempDir()}}
	if d.desktopEnabled() {
		t.Fatal("must not enable automatically")
	}
	if err := d.saveDesktopEnabled(true); err != nil {
		t.Fatal(err)
	}
	if !d.desktopEnabled() {
		t.Fatal("enable was not persisted")
	}
	if err := d.saveDesktopEnabled(false); err != nil {
		t.Fatal(err)
	}
	if d.desktopEnabled() {
		t.Fatal("stop was not persisted")
	}
}
func TestDesktopAddDomainPreservesExistingList(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "list-general-user.txt")
	if err := os.WriteFile(path, []byte("original.example\n"), 0600); err != nil {
		t.Fatal(err)
	}
	d := &Daemon{dirs: dirs{lists: dir}}
	if err := d.AddDomain("new.example"); err != nil {
		t.Fatal(err)
	}
	if err := d.AddDomain("new.example"); err != nil {
		t.Fatal(err)
	}
	b, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if string(b) != "original.example\nnew.example\n" {
		t.Fatal(string(b))
	}
}
