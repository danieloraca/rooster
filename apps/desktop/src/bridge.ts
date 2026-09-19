import { invoke } from '@tauri-apps/api/core';
import type { Bridge } from './types';
export const nativeBridge: Bridge = {
  edit: (action,args) => invoke('editing', {request:{action,...args}}),
  chooseDraftSource: () => invoke('choose_draft_source'),
  settings: () => invoke('load_settings'),
  chooseWorkspace: name => invoke('choose_workspace', { name }),
  removeLocation: rootId => invoke('remove_location', { rootId }),
  start: request => invoke('start_scan', { request }),
  status: () => invoke('scan_status'),
  cancel: ticket => invoke('cancel_scan', { ticket }),
  inventory: generation => invoke('get_inventory', { generation }),
  inspect: (generation,id) => invoke('inspect_artifact', { generation,id }),
  assess: (generation,id) => invoke('assess_context', { generation,id }),
  search: (generation,query) => invoke('search_artifacts', { generation,query }),
};
