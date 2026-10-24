import http from 'node:http';
import type { IndexerDB } from './db';

export function periodArchetypes(db: IndexerDB, contractId: string, period: number) {
  const byArch = db.getArchetypeCounts(contractId).get(period) ?? new Map<string, number>();
  const total = [...byArch.values()].reduce((a, b) => a + b, 0);
  const archetypes = [...byArch.entries()]
    .map(([archetype, count]) => ({ archetype, count, share: total ? count / total : 0 }))
    .sort((a, b) => b.count - a.count || a.archetype.localeCompare(b.archetype));
  return { period, total, archetypes };
}

export function archetypeRarity(db: IndexerDB, contractId: string, archetype: string) {
  const periods: { period: number; count: number; total: number; share: number }[] = [];
  for (const [period, byArch] of db.getArchetypeCounts(contractId)) {
    const count = byArch.get(archetype);
    if (!count) continue;
    const total = [...byArch.values()].reduce((a, b) => a + b, 0);
    periods.push({ period, count, total, share: count / total });
  }
  periods.sort((a, b) => a.period - b.period);
  return { archetype, periods };
}

export function createApiServer(db: IndexerDB, contractId: string): http.Server {
  return http.createServer((req, res) => {
    const send = (status: number, body: unknown) => {
      res.writeHead(status, { 'content-type': 'application/json' });
      res.end(JSON.stringify(body));
    };
    if (req.method !== 'GET') return send(405, { error: 'method not allowed' });

    const path = new URL(req.url ?? '/', 'http://localhost').pathname;

    let m = path.match(/^\/periods\/([^/]+)\/archetypes$/);
    if (m) {
      const period = Number(m[1]);
      if (!Number.isInteger(period) || period < 0) {
        return send(400, { error: 'invalid period' });
      }
      return send(200, periodArchetypes(db, contractId, period));
    }

    m = path.match(/^\/archetypes\/([^/]+)\/rarity$/);
    if (m) {
      let id: string;
      try {
        id = decodeURIComponent(m[1]);
      } catch {
        return send(400, { error: 'invalid archetype id' });
      }
      return send(200, archetypeRarity(db, contractId, id));
    }

    send(404, { error: 'not found' });
  });
}
