#!/usr/bin/env python3
"""Generate a new, deterministic fixture for measure_discovery; never use a real checkout."""
import argparse
import json
from pathlib import Path
import subprocess

p = argparse.ArgumentParser(description=__doc__)
p.add_argument("directory", type=Path)
p.add_argument("--repositories", type=int, default=10)
p.add_argument("--documents", type=int, default=500, help="Markdown files per repository")
a = p.parse_args()
if not 1 <= a.repositories <= 100 or not 1 <= a.documents <= 20000:
    p.error("use 1–100 repositories and 1–20000 documents per repository")
a.directory.mkdir(parents=True, exist_ok=False)
base = a.directory.resolve()
for i in range(a.repositories):
    repo = base / "repositories" / f"repo-{i:03}"
    repo.mkdir(parents=True)
    subprocess.run(["git", "init", "--quiet", "--template=", "--initial-branch=main", str(repo)], check=True)
    (repo / "AGENTS.md").write_text("# Guidance\nRun focused tests.\n")
    (repo / "CLAUDE.md").write_text("# Guidance\nRun focused tests.\n")
    for j in range(a.documents):
        d = repo / "docs" / f"module-{j // 10:04}"
        d.mkdir(exist_ok=True, parents=True)
        (d / f"guide-{j:05}.md").write_text("# Guide\n" + "Ordinary project documentation.\n" * 30)
        if j % 10 == 0:
            excluded = repo / "node_modules" / f"pkg-{j:05}"
            excluded.mkdir(parents=True)
            (excluded / "AGENTS.md").write_text("Excluded fixture\n")
config = {"schema_version":1,"next_id":3,"workspaces":[{"id":"workspace-1","name":"Benchmark","roots":[{"id":"root-2","path":str(base / 'repositories')}]}],"excluded_dirs":["node_modules","vendor","target","dist","build"],"codex":{"include_default_roots":False},"claude":{"include_default_roots":False}}
(base / "config.json").write_text(json.dumps(config, indent=2) + "\n")
print(base / "config.json")
