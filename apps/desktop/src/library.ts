import type { Artifact, Kind, NativePath, SkillPackage } from './types';

export type Category = 'all' | 'instruction' | 'skill' | 'agent' | 'settings';
export function matchesCategory(kind: Kind, category: Category): boolean {
  return category === 'all' || kind === category
    || (category === 'instruction' && kind === 'rule')
    || (category === 'skill' && kind === 'legacy_command')
    || (category === 'agent' && kind === 'legacy_agent')
    || (category === 'settings' && ['provider_config', 'plugin_manifest'].includes(kind));
}
export interface LibraryRow {
  artifact: Artifact;
  children: Artifact[];
  packageRoot: NativePath | null;
}

export function rankLibraryRows(rows: LibraryRow[], searchRank: Map<string, number> | null): LibraryRow[] {
  if (searchRank === null) return rows;
  const rank = (row: LibraryRow) => Math.min(
    searchRank.get(row.artifact.id) ?? Infinity,
    ...row.children.map(child => searchRank.get(child.id) ?? Infinity),
  );
  return [...rows].sort((left, right) => rank(left) - rank(right));
}

// Build the hierarchy from package identities, never filenames or skill names.
// A matching child retains its parent as context even when the parent did not match.
export function libraryRows(artifacts: Artifact[], packages: SkillPackage[], category: Category,
  accepts: (artifact: Artifact) => boolean): { rows: LibraryRow[]; count: number } {
  const grouped = category === 'all' || category === 'skill';
  const byId = new Map(artifacts.map(artifact => [artifact.id, artifact]));
  const parents = new Map<string, { artifact: Artifact; root: NativePath }>();
  if (grouped) for (const pkg of packages) {
    const skill = byId.get(pkg.skill_id);
    if (!skill || skill.kind !== 'skill') continue;
    for (const id of pkg.member_ids) {
      if (id !== skill.id && byId.get(id)?.package_id === pkg.id) parents.set(id, { artifact: skill, root: pkg.root });
    }
  }
  const rows = new Map<string, LibraryRow>();
  let count = 0;
  for (const artifact of artifacts) {
    const parent = parents.get(artifact.id);
    const included = matchesCategory(artifact.kind, category)
      || (category === 'skill' && (parent !== undefined || artifact.kind === 'skill_metadata'));
    if (!included || !accepts(artifact)) continue;
    count++;
    const owner = parent?.artifact ?? artifact;
    let row = rows.get(owner.id);
    if (!row) {
      row = { artifact: owner, children: [], packageRoot: parent?.root ?? null };
      rows.set(owner.id, row);
    }
    if (parent) { row.packageRoot = parent.root; row.children.push(artifact); }
  }
  for (const row of rows.values()) row.children.sort((a, b) => Number(b.kind === 'skill_metadata') - Number(a.kind === 'skill_metadata'));
  return { rows: [...rows.values()], count };
}
