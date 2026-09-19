#!/usr/bin/env python3
"""Repeat the two-layout pilot in a NEW disposable directory; no provider session runs."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess


def run(argv, **kwargs):
    result = subprocess.run([str(a) for a in argv], capture_output=True, text=True, **kwargs)
    if result.returncode:
        raise RuntimeError(f"{argv}: {result.stderr}")
    return result.stdout


def git(repo, *args):
    return run(["git", "-C", repo, "-c", "core.hooksPath=/dev/null", *args])


def put(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)


def tree(repo):
    return {str(p.relative_to(repo)): hashlib.sha256(p.read_bytes()).hexdigest()
            for p in repo.rglob("*") if p.is_file() and ".git" not in p.relative_to(repo).parts}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--directory", required=True, type=Path, help="Must not already exist")
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    base = args.directory.absolute()
    base.mkdir(parents=True, exist_ok=False)
    base = base.resolve()
    layouts = []
    for label, relative, parent_root in [("A", "developer-a/projects", True), ("B", "developer-b/Volumes/Work ü/clients", False)]:
        projects = base / relative
        state = base / f"private-{label}"
        state.mkdir(mode=0o700)
        config = state / "config.json"
        config.write_text(json.dumps({"schema_version": 1, "next_id": 1, "workspaces": [],
            "excluded_dirs": ["node_modules", "vendor", "target", "dist", "build"],
            "codex": {"include_default_roots": False}, "claude": {"include_default_roots": False}}))
        config.chmod(0o600)
        env = dict(os.environ, ROOSTER_CONFIG=str(config), ROOSTER_DATA_DIR=str(state / "recovery"))
        def cli(*cmd, code=0):
            result = subprocess.run([str(binary), *map(str, cmd)], env=env, capture_output=True, text=True)
            assert result.returncode == code, (cmd, result.returncode, result.stderr)
            return json.loads(result.stdout)
        repos = [projects / "Orchard-Web", projects / "Orchard-API"]
        for repo in repos:
            repo.mkdir(parents=True)
            git(repo, "init", "--quiet", "--template=", "--initial-branch=main")
            put(repo / "AGENTS.md", "# Team guidance\nReview changes and run focused tests.\n")
            put(repo / "CLAUDE.md", "# Team guidance\nReview changes and run focused tests.\n")
            put(repo / "App/.agents/skills/review/SKILL.md", "---\nname: review\ndescription: Review application changes.\n---\nRead [checklist](references/check.md).\n")
            put(repo / "App/.agents/skills/review/references/check.md", "# Checklist\nCheck ownership and recovery.\n")
            put(repo / ".claude/agents/review/security.md", "---\nname: security\ndescription: Review security boundaries.\n---\nInspect the change.\n")
            git(repo, "add", ".")
            git(repo, "-c", "user.name=Rooster Pilot", "-c", "user.email=pilot@example.invalid", "-c", "commit.gpgsign=false", "commit", "--quiet", "-m", "Disposable pilot guidance")
        registration = cli("workspace", "add", "Pilot", projects if parent_root else repos[0], "--json")
        if not parent_root:
            cli("workspace", "add", "Pilot", repos[1], "--json")
        checkouts = cli("scan", "--json")["checkouts"]
        assert len(checkouts) == 2
        before = [tree(repo) for repo in repos]
        assert before[0] == before[1]
        inventories = {provider: cli("artifacts", "list", "--provider", provider, "--all-markdown", "--no-default-roots", "--json") for provider in ("codex", "claude")}
        assert all(i["status"] == "complete" for i in inventories.values())
        for provider, filename in [("codex", "AGENTS.md"), ("claude", "CLAUDE.md")]:
            artifact = next(a for a in inventories[provider]["artifacts"] if a["path"] == str(repos[0] / filename))
            source = state / "proposal.md"
            source.write_text((repos[0] / filename).read_text() + "Keep personal drafts outside the checkout.\n")
            preview = cli("changes", "--provider", provider, "edit", artifact["id"], "--source", source)
            assert tree(repos[0]) == before[0]
            assert cli("changes", "apply", preview["id"])["status"] == "completed"
            assert git(repos[0], "diff", "--name-only").strip() == filename
            diff = git(repos[0], "diff", "--", filename)
            assert "+Keep personal drafts" in diff
            put(state / f"{provider}-review.diff", diff)
            assert cli("changes", "restore", preview["id"])["status"] == "restored"
            assert tree(repos[0]) == before[0]
        # A second clone with the same commits remains a distinct owner.
        second = projects / "Orchard-Web-second"
        run(["git", "-c", "core.hooksPath=/dev/null", "clone", "--quiet", "--no-hardlinks", repos[0], second])
        if not parent_root:
            cli("workspace", "add", "Pilot", second, "--json")
        discovered = cli("scan", "--json")["checkouts"]
        assert len(discovered) == 3 and len({c["id"] for c in discovered}) == 3
        # Offlining a registered root retains registration and yields a partial result.
        original_root = projects if parent_root else repos[0]
        moved = original_root.with_name(original_root.name + "-moved")
        original_root.rename(moved)
        partial = cli("scan", "--json", code=2)
        assert partial["status"] == "partial" and partial["issues"]
        root_id = registration["root"]["id"]
        assert cli("workspace", "relocate", root_id, moved, "--json")["root"]["id"] == root_id
        relocated = cli("scan", "--json")
        assert relocated["status"] == "complete" and len(relocated["checkouts"]) == 3
        old_ids = {c["id"] for c in discovered}
        assert any(c["id"] not in old_ids for c in relocated["checkouts"])
        # Return to an easy-to-use, clean native-UI fixture layout.
        moved.rename(original_root)
        cli("workspace", "relocate", root_id, original_root, "--json")
        for repo in [*repos, second]:
            assert not git(repo, "status", "--porcelain", "--untracked-files=all").strip()
            assert not state.is_relative_to(repo)
        layouts.append({"label": label, "config": str(config), "data": str(state / "recovery"), "projects": str(projects), "repositories": [str(r) for r in repos], "guidance_hashes": before[0], "status": "passed"})
    assert layouts[0]["guidance_hashes"] == layouts[1]["guidance_hashes"]
    result = {"kind": "two isolated local developer layouts; not a human teammate trial", "layouts": layouts, "checks": ["parent and direct registrations", "Codex and Claude inventory", "review/apply/Git diff/restore", "private recovery and settings", "second clone ownership", "offline and relocated root"], "status": "passed"}
    (base / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
