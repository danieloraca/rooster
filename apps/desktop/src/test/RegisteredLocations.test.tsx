import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeAll, expect, it, vi } from 'vitest';
import RegisteredLocations from '../RegisteredLocations';
import type { Workspace } from '../types';

afterEach(cleanup);
beforeAll(() => {
  HTMLDialogElement.prototype.showModal = function() { this.open = true; };
  HTMLDialogElement.prototype.close = function() { this.open = false; };
});
const workspaces: Workspace[] = [
  { id: 'work', name: 'Work', roots: [{ id: 'root-one', path: '/offline/Repos ü' }] },
  { id: 'other', name: 'Other', roots: [{ id: 'root-two', path: '/offline/Repos ü' }] },
];
it('shows registered offline folders, scopes removal by ID, and requires confirmation', async () => {
  const onRemove = vi.fn(async () => {});
  const { rerender } = render(<RegisteredLocations workspaces={workspaces} workspaceId={null} busy={false} onRemove={onRemove}/>);
  expect(screen.getAllByRole('button', { name: /^Remove location / })).toHaveLength(2);
  fireEvent.click(screen.getByRole('button', { name: 'Remove location /offline/Repos ü from Work' }));
  expect(screen.getByText(/files and repositories stay/)).toBeVisible();
  expect(screen.getByText(/last location in Work/)).toBeVisible();
  fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
  expect(onRemove).not.toHaveBeenCalled();
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', { name: 'Remove location /offline/Repos ü from Other' }));
  fireEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Remove location' }));
  await waitFor(() => expect(onRemove).toHaveBeenCalledWith('root-two'));
  await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
  rerender(<RegisteredLocations workspaces={workspaces} workspaceId="work" busy={false} onRemove={onRemove}/>);
  expect(screen.getAllByRole('button', { name: /^Remove location / })).toHaveLength(1);
});
it('keeps errors visible and allows a failed removal to be retried', async () => {
  const onRemove = vi.fn().mockRejectedValueOnce(new Error('Settings could not be saved')).mockResolvedValueOnce(undefined);
  render(<RegisteredLocations workspaces={workspaces} workspaceId="work" busy={false} onRemove={onRemove}/>);
  fireEvent.click(screen.getByRole('button', { name: /^Remove location / }));
  fireEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Remove location' }));
  expect(await screen.findByRole('alert')).toHaveTextContent('Settings could not be saved');
  expect(screen.getByRole('dialog')).toBeVisible();
  fireEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Remove location' }));
  await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
  expect(onRemove).toHaveBeenCalledTimes(2);
});
