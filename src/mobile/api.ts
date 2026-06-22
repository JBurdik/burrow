export type SessionStatus =
  | 'idle'
  | 'running'
  | 'waiting'
  | 'permission'
  | 'done'
  | 'review'
  | 'error';

export interface Session {
  ptyId: number;
  title: string;
  status: SessionStatus;
  statusDetail?: string;
  workspaceId: number;
  workspaceName: string;
  cwd: string;
  model?: string;
}

export interface Workspace {
  id: number;
  name: string;
  path: string;
  sessions: Session[];
}

export interface RemoteConfig {
  baseUrl: string;
  token: string;
}

export class BurrowClient {
  constructor(private cfg: RemoteConfig) {}

  private headers(): HeadersInit {
    return { Authorization: `Bearer ${this.cfg.token}` };
  }

  async listWorkspaces(): Promise<Workspace[]> {
    const res = await fetch(`${this.cfg.baseUrl}/api/workspaces`, {
      headers: this.headers(),
      signal: AbortSignal.timeout(8000),
    });
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    return res.json();
  }

  async pair(code: string): Promise<string> {
    const res = await fetch(`${this.cfg.baseUrl}/api/pair`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ code }),
      signal: AbortSignal.timeout(8000),
    });
    const payload = await res.json().catch(() => ({}));
    if (!res.ok || typeof payload.token !== 'string') {
      throw new Error(payload.error ?? `HTTP ${res.status}`);
    }
    return payload.token;
  }

  async getOutput(ptyId: number): Promise<string> {
    const res = await fetch(`${this.cfg.baseUrl}/api/output/${ptyId}`, {
      headers: this.headers(),
      signal: AbortSignal.timeout(8000),
    });
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    return res.text();
  }

  async sendInput(ptyId: number, text: string): Promise<void> {
    const res = await fetch(`${this.cfg.baseUrl}/api/sessions/${ptyId}/input`, {
      method: 'POST',
      headers: { ...this.headers(), 'Content-Type': 'application/json' },
      body: JSON.stringify({ text }),
      signal: AbortSignal.timeout(8000),
    });
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
  }

  async interrupt(ptyId: number): Promise<void> {
    const res = await fetch(`${this.cfg.baseUrl}/api/sessions/${ptyId}/interrupt`, {
      method: 'POST',
      headers: this.headers(),
      signal: AbortSignal.timeout(8000),
    });
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
  }
}
