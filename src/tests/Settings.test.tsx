import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { Settings } from '../components/Settings';

const mockInvoke = vi.hoisted(() => vi.fn());
const mockListen = vi.hoisted(() => vi.fn().mockResolvedValue(vi.fn()));
const mockGetVersion = vi.hoisted(() => vi.fn());

vi.mock('@tauri-apps/api/core', () => ({
  invoke: mockInvoke,
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: mockListen,
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-opener', () => ({
  openUrl: vi.fn(),
}));

vi.mock('@tauri-apps/api/app', () => ({
  getVersion: mockGetVersion,
}));

describe('Settings', () => {
  const defaultSettings = {
    hotkey: 'CmdOrCtrl+Shift+Space',
    recording_mode: 'toggle' as const,
    active_model: 'base',
    language: 'auto',
    auto_paste: true,
    max_recording_seconds: 120,
    launch_at_login: false,
    overlay_position: 'top_center' as const,
    lower_volume_while_recording: true,
  };

  beforeEach(() => {
    vi.clearAllMocks();
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_settings') return Promise.resolve(defaultSettings);
      if (cmd === 'get_launch_at_login') return Promise.resolve(false);
      if (cmd === 'check_accessibility') return Promise.resolve(true);
      return Promise.reject(new Error(`Unknown command: ${cmd}`));
    });
    mockGetVersion.mockResolvedValue('0.4.3');
  });

  it('renders settings form with default values', async () => {
    render(<Settings />);
    
    await waitFor(() => {
      expect(screen.getByText('Careless Whisper')).toBeInTheDocument();
    });
    
    expect(screen.getByDisplayValue('CmdOrCtrl+Shift+Space')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Save Settings' })).toBeInTheDocument();
  });

  it('shows app version', async () => {
    render(<Settings />);
    
    await waitFor(() => {
      expect(screen.getByText('v0.4.3')).toBeInTheDocument();
    });
  });

  it('hides accessibility banner when granted', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_settings') return Promise.resolve(defaultSettings);
      if (cmd === 'get_launch_at_login') return Promise.resolve(false);
      if (cmd === 'check_accessibility') return Promise.resolve(true);
      return Promise.reject(new Error(`Unknown command: ${cmd}`));
    });
    
    render(<Settings />);
    
    await waitFor(() => {
      expect(screen.queryByText('Accessibility Permission Required')).not.toBeInTheDocument();
    });
  });

  it('shows accessibility banner when not granted', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_settings') return Promise.resolve(defaultSettings);
      if (cmd === 'get_launch_at_login') return Promise.resolve(false);
      if (cmd === 'check_accessibility') return Promise.resolve(false);
      return Promise.reject(new Error(`Unknown command: ${cmd}`));
    });
    
    render(<Settings />);
    
    await waitFor(() => {
      expect(screen.getByText('Accessibility Permission Required')).toBeInTheDocument();
    });
  });

  it('updates hotkey value on change', async () => {
    render(<Settings />);
    
    const hotkeyInput = await waitFor(() => screen.getByDisplayValue('CmdOrCtrl+Shift+Space'));
    fireEvent.change(hotkeyInput, { target: { value: 'Ctrl+Shift+V' } });
    
    expect(screen.getByDisplayValue('Ctrl+Shift+V')).toBeInTheDocument();
  });

  it('updates recording mode on change', async () => {
    render(<Settings />);
    
    const selects = await waitFor(() => screen.getAllByRole('combobox'));
    const recordingModeSelect = selects[0];
    fireEvent.change(recordingModeSelect, { target: { value: 'push_to_talk' } });
    
    const option = screen.getByRole('option', { name: 'Push to Talk (hold to record)' });
    expect((option as HTMLOptionElement).selected).toBe(true);
  });

  it('calls update_settings when save is clicked', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_settings') return Promise.resolve(defaultSettings);
      if (cmd === 'get_launch_at_login') return Promise.resolve(false);
      if (cmd === 'check_accessibility') return Promise.resolve(true);
      if (cmd === 'update_settings') return Promise.resolve();
      return Promise.reject(new Error(`Unknown command: ${cmd}`));
    });
    
    render(<Settings />);
    
    await waitFor(() => {
      expect(screen.getByRole('button', { name: 'Save Settings' })).toBeInTheDocument();
    });
    
    fireEvent.click(screen.getByRole('button', { name: 'Save Settings' }));
    
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('update_settings', { settings: expect.any(Object) });
    });
  });

  it('shows saved confirmation after save', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_settings') return Promise.resolve(defaultSettings);
      if (cmd === 'get_launch_at_login') return Promise.resolve(false);
      if (cmd === 'check_accessibility') return Promise.resolve(true);
      if (cmd === 'update_settings') return Promise.resolve();
      return Promise.reject(new Error(`Unknown command: ${cmd}`));
    });
    
    render(<Settings />);
    
    await waitFor(() => {
      expect(screen.getByRole('button', { name: 'Save Settings' })).toBeInTheDocument();
    });
    
    fireEvent.click(screen.getByRole('button', { name: 'Save Settings' }));
    
    await waitFor(() => {
      expect(screen.getByRole('button', { name: 'Saved!' })).toBeInTheDocument();
    }, { timeout: 3000 });
  });
});
