import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import ArtifactList from '../ArtifactList';
import { libraryRows } from '../library';
import type { Artifact, Kind, SkillPackage } from '../types';

afterEach(cleanup);
function file(id: string, kind: Kind, path: string, packageId: string | null = null): Artifact {
  return { id, kind, path, physical_path: path, package_id: packageId, provider: 'codex',
    owner: { id: 'repo', root: '/repo', checkout_id: 'repo', head: null, branch: 'main' },
    provenance: 'repository', source_root: '/repo', scope_directory: '/repo', scope_checkout_id: 'repo',
    name: kind === 'skill' ? id : null, description: null, declared_role_names: [], role_declaration_count: 0,
    validation: 'valid', unknown_fields: [], read_only_reason: null, snapshot: null, ignored: false, references: [] };
}
const alpha = file('alpha', 'skill', '/repo/.agents/skills/alpha/SKILL.md', 'package-a');
const alphaMetadata = file('alpha-yaml', 'skill_metadata', '/repo/.agents/skills/alpha/agents/openai.yaml', 'package-a');
const alphaReference = file('alpha-guide', 'reference', '/repo/.agents/skills/alpha/references/guide.md', 'package-a');
const alphaScript = file('alpha-script', 'supporting_file', '/repo/.agents/skills/alpha/scripts/check.sh', 'package-a');
const beta = file('beta', 'skill', '/repo/.agents/skills/beta/SKILL.md', 'package-b');
const betaMetadata = file('beta-yaml', 'skill_metadata', '/repo/.agents/skills/beta/agents/openai.yaml', 'package-b');
const config = file('config', 'provider_config', '/repo/.codex/config.toml');
const manifest = file('manifest', 'plugin_manifest', '/repo/.codex-plugin/plugin.json');
const artifacts = [alpha, alphaMetadata, alphaReference, alphaScript, beta, betaMetadata, config, manifest];
const packages: SkillPackage[] = [
  { id: 'package-a', root: '/repo/.agents/skills/alpha', physical_root: '/repo/.agents/skills/alpha', skill_id: alpha.id, member_ids: [alpha.id, alphaMetadata.id, alphaReference.id, alphaScript.id] },
  { id: 'package-b', root: '/repo/.agents/skills/beta', physical_root: '/repo/.agents/skills/beta', skill_id: beta.id, member_ids: [beta.id, betaMetadata.id] },
];
it('keeps provider and plugin configuration in Settings without skill metadata', () => {
  const { rows, count } = libraryRows(artifacts, packages, 'settings', () => true);
  expect(rows.map(row => row.artifact.id)).toEqual(['config', 'manifest']);
  expect(count).toBe(2);
});
it('expands metadata, references, and scripts under their own skill and selects exact IDs', () => {
  const onSelect = vi.fn();
  const { rows, count } = libraryRows(artifacts, packages, 'skill', () => true);
  expect(count).toBe(6);
  render(<ArtifactList rows={rows} selected={null} filtered={false} onSelect={onSelect}/>);
  expect(screen.queryByText('agents/openai.yaml')).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', { name: 'Expand files for alpha' }));
  const alphaGroup = within(screen.getByRole('region', { name: 'alpha skill package' }));
  expect(alphaGroup.getByText('references/guide.md')).toBeVisible();
  expect(alphaGroup.getByText('scripts/check.sh')).toBeVisible();
  fireEvent.click(alphaGroup.getByRole('button', { name: /^agents\/openai.yaml/ }));
  expect(onSelect).toHaveBeenLastCalledWith('alpha-yaml');
  fireEvent.click(screen.getByRole('button', { name: 'Expand files for beta' }));
  const betaGroup = within(screen.getByRole('region', { name: 'beta skill package' }));
  fireEvent.click(betaGroup.getByRole('button', { name: /^agents\/openai.yaml/ }));
  expect(onSelect).toHaveBeenLastCalledWith('beta-yaml');
  fireEvent.click(screen.getByRole('button', { name: 'Collapse files for alpha' }));
  expect(alphaGroup.queryByText('agents/openai.yaml')).not.toBeInTheDocument();
});
it('shows a matching child with its parent during search even when the parent does not match', () => {
  const { rows, count } = libraryRows(artifacts, packages, 'all', artifact => artifact.id === 'beta-yaml');
  expect(count).toBe(1);
  render(<ArtifactList rows={rows} selected={null} filtered onSelect={vi.fn()}/>);
  expect(screen.getByRole('region', { name: 'beta skill package' })).toHaveTextContent('agents/openai.yaml');
  expect(screen.queryByText('alpha')).not.toBeInTheDocument();
  expect(screen.getByText('1 matching package file')).toBeVisible();
});
it('keeps malformed child metadata reachable in Needs attention', () => {
  const malformed: Artifact = { ...alphaMetadata, validation: 'malformed' };
  const { rows } = libraryRows([alpha, malformed, beta, betaMetadata], packages, 'skill', artifact => artifact.validation !== 'valid');
  render(<ArtifactList rows={rows} selected={null} filtered onSelect={vi.fn()}/>);
  expect(screen.getByRole('region', { name: 'alpha skill package' })).toHaveTextContent('agents/openai.yaml');
  expect(screen.getByLabelText('malformed')).toBeVisible();
  expect(screen.queryByText('beta')).not.toBeInTheDocument();
});
it('expands a package when a related-file link selects a child', () => {
  const { rows } = libraryRows(artifacts, packages, 'all', () => true);
  const { rerender } = render(<ArtifactList rows={rows} selected={null} filtered={false} onSelect={vi.fn()}/>);
  expect(screen.queryByText('agents/openai.yaml')).not.toBeInTheDocument();
  rerender(<ArtifactList rows={rows} selected="alpha-yaml" filtered={false} onSelect={vi.fn()}/>);
  expect(screen.getByRole('button', { name: /^agents\/openai.yaml/ })).toHaveAttribute('aria-pressed', 'true');
});
it('does not lose metadata when a partial or scoped inventory lacks its parent', () => {
  const { rows, count } = libraryRows([alpha, betaMetadata], packages, 'all', () => true);
  expect(count).toBe(2);
  expect(rows.map(row => row.artifact.id)).toEqual(['alpha', 'beta-yaml']);
  expect(rows.every(row => row.children.length === 0)).toBe(true);
});
