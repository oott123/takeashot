---
trigger: always_on
---

OS: NixOS

Shell: zsh, in a nix flakes powered dev shell

Install tools or Python dependencies: edit `flake.nix` and adds packages there. Run `direnv reload` to reload the shell after edit. Packages should get installed automatically then.

Running python scripts: just `python file.py`

Running temporary scripts for debugging: write to `prototypes/` directory, use `python prototypes/file.py` to run it.

Running python directly in the shell: use python -c with heredoc.
