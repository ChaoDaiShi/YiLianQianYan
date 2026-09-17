export class RefreshGate {
  private busy = false;
  private last = -Infinity;
  async run(now: number, visible: boolean, refresh: () => Promise<void>): Promise<boolean> {
    if (this.busy || !visible || now - this.last < 30_000) return false;
    this.busy = true; this.last = now;
    try { await refresh(); return true; } finally { this.busy = false; }
  }
}
