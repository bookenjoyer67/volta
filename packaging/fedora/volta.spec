Name:           volta
Version:        0.1.0
Release:        1%{?dist}
Summary:        Desktop ebook reader with RSVP speed reading (TUI + GUI)

License:        MIT
URL:            https://git.komun.buzz/Book-Enjoyer/volta
Source0:        https://git.komun.buzz/Book-Enjoyer/volta/archive/v%{version}.tar.gz

BuildRequires:  cargo
BuildRequires:  rust
BuildRequires:  gcc

Requires:       love
Requires:       poppler-utils
Requires:       zenity
Requires:       curl

%description
Volta is a dual-mode ebook reader supporting EPUB, PDF, and Markdown.
It features RSVP (Rapid Serial Visual Presentation) speed reading,
a keyboard-driven TUI (terminal) mode, and a LÖVE-based GUI mode.

Features:
 * Library grid with cover images and progress bars
 * RSVP one-word-at-a-time reading with adjustable WPM
 * Full-text search across all chapters
 * 8 built-in color themes
 * Progress saving and resume
 * URL auto-download for remote books

%prep
%setup -q -n volta

%build
cargo build --release --manifest-path core/Cargo.toml

%check
cargo test --release --manifest-path core/Cargo.toml

%install
# TUI binary
install -Dm755 target/release/volta-tui %{buildroot}%{_bindir}/volta-tui

# Shared library for LÖVE
install -Dm755 target/release/libvolta_core.so %{buildroot}%{_libdir}/volta/libvolta_core.so

# LÖVE frontend
install -dm755 %{buildroot}%{_datadir}/volta/frontend
cp -r frontend/* %{buildroot}%{_datadir}/volta/frontend/

# Launcher script
install -Dm755 volta %{buildroot}%{_bindir}/volta

# Desktop entry
sed 's|Exec=.*|Exec=%{_bindir}/volta|' volta.desktop > volta.desktop.rpm
install -Dm644 volta.desktop.rpm %{buildroot}%{_datadir}/applications/volta.desktop
rm -f volta.desktop.rpm

# Docs
install -Dm644 README.md %{buildroot}%{_docdir}/volta/README.md
install -Dm644 KEYBINDINGS.md %{buildroot}%{_docdir}/volta/KEYBINDINGS.md

# License
install -Dm644 LICENSE %{buildroot}%{_defaultlicensedir}/volta/LICENSE

%files
%license LICENSE
%doc README.md KEYBINDINGS.md
%{_bindir}/volta
%{_bindir}/volta-tui
%{_libdir}/volta/libvolta_core.so
%{_datadir}/volta/frontend/
%{_datadir}/applications/volta.desktop

%changelog
* Thu Jul 24 2026 Book-Enjoyer <catperson@catperson.online> - 0.1.0-1
- Initial release
