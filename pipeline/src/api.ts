import Fastify, { type FastifyInstance, type FastifyReply, type FastifyRequest } from 'fastify';
import type { IndexerDB } from './db';
import type { WrapRecord } from './types';

// ─── Read API ──────────────────────────────────────────────────────────
//
// Exposes the derived indexer state over HTTP so clients stop scanning the
// chain with unbounded contract queries. Every response carries the ledger
// sequence it is consistent with (`ledger_seq`) so clients can detect lag.

export type WrapPeriod = 'all' | 'day' | 'week' | 'month';

export interface ApiOptions {
  db: IndexerDB;
  /** Returns the current chain head ledger, used by /health. */
  getChainHead?: () => Promise<number>;
  /** Max wraps returned per page. Defaults to 50. */
  pageSize?: number;
  /** Rate limit: max requests per window per client. Defaults to 120. */
  rateLimitMax?: number;
  /** Rate limit window in ms. Defaults to 60_000. */
  rateLimitWindowMs?: number;
}

interface WrapQuery {
  cursor?: string;
  limit?: string;
}

interface AddressParams {
  address: string;
}

interface PeriodParams extends AddressParams {
  period: string;
}

const PERIOD_MS: Record<Exclude<WrapPeriod, 'all'>, number> = {
  day: 24 * 60 * 60 * 1000,
  week: 7 * 24 * 60 * 60 * 1000,
  month: 30 * 24 * 60 * 60 * 1000,
};

function parseLimit(raw: string | undefined, fallback: number): number {
  const n = Number(raw);
  if (!Number.isFinite(n) || n <= 0) return fallback;
  return Math.min(Math.floor(n), fallback);
}

function parseCursor(raw: string | undefined): number {
  const n = Number(raw);
  return Number.isFinite(n) && n >= 0 ? Math.floor(n) : 0;
}

function isPeriod(value: string): value is WrapPeriod {
  return value === 'all' || value === 'day' || value === 'week' || value === 'month';
}

/**
 * Build the Fastify instance exposing the indexer read API. Kept separate
 * from the server bootstrap so it can be exercised directly in tests.
 */
export function buildApi(opts: ApiOptions): FastifyInstance {
  const { db, getChainHead } = opts;
  const pageSize = opts.pageSize ?? 50;
  const rateLimitMax = opts.rateLimitMax ?? 120;
  const rateLimitWindowMs = opts.rateLimitWindowMs ?? 60_000;

  const app = Fastify({ logger: false });

  // ── Simple in-memory rate limiting ──────────────────────────────────
  const hits = new Map<string, { count: number; resetAt: number }>();
  app.addHook('onRequest', async (req: FastifyRequest, reply: FastifyReply) => {
    const key = req.ip;
    const now = Date.now();
    const entry = hits.get(key);
    if (!entry || entry.resetAt <= now) {
      hits.set(key, { count: 1, resetAt: now + rateLimitWindowMs });
      return;
    }
    entry.count += 1;
    if (entry.count > rateLimitMax) {
      reply.header('retry-after', Math.ceil((entry.resetAt - now) / 1000));
      return reply.code(429).send({ error: 'rate_limited' });
    }
  });

  // ── ETag caching ────────────────────────────────────────────────────
  // Responses are deterministic for a given ledger, so the ledger sequence
  // doubles as a strong ETag. Clients revalidate cheaply with If-None-Match.
  function sendWithEtag(reply: FastifyReply, ledgerSeq: number, body: unknown) {
    const etag = `"ledger-${ledgerSeq}"`;
    reply.header('etag', etag);
    reply.header('cache-control', 'public, max-age=5');
    const inm = reply.request.headers['if-none-match'];
    if (inm === etag) {
      return reply.code(304).send();
    }
    return reply.send(body);
  }

  async function currentLedger(): Promise<number> {
    return db.getLastLedger();
  }

  async function loadWraps(address: string): Promise<WrapRecord[]> {
    return db.getWrapsForUser(address);
  }

  function filterByPeriod(wraps: WrapRecord[], period: WrapPeriod): WrapRecord[] {
    if (period === 'all') return wraps;
    const cutoff = Date.now() - PERIOD_MS[period];
    return wraps.filter((w) => {
      const ts = Number((w as any).timestamp ?? (w as any).created_at ?? 0);
      return ts >= cutoff;
    });
  }

  function paginate(wraps: WrapRecord[], cursor: number, limit: number) {
    const slice = wraps.slice(cursor, cursor + limit);
    const nextCursor = cursor + limit < wraps.length ? cursor + limit : null;
    return { wraps: slice, next_cursor: nextCursor };
  }

  // ── GET /wraps/:address ─────────────────────────────────────────────
  app.get<{ Params: AddressParams; Querystring: WrapQuery }>(
    '/wraps/:address',
    async (req, reply) => {
      const ledgerSeq = await currentLedger();
      const wraps = await loadWraps(req.params.address);
      const limit = parseLimit(req.query.limit, pageSize);
      const cursor = parseCursor(req.query.cursor);
      const page = paginate(wraps, cursor, limit);
      return sendWithEtag(reply, ledgerSeq, {
        address: req.params.address,
        ledger_seq: ledgerSeq,
        total: wraps.length,
        ...page,
      });
    },
  );

  // ── GET /wraps/:address/:period ─────────────────────────────────────
  app.get<{ Params: PeriodParams; Querystring: WrapQuery }>(
    '/wraps/:address/:period',
    async (req, reply) => {
      const { period } = req.params;
      if (!isPeriod(period)) {
        return reply.code(400).send({ error: 'invalid_period', allowed: ['all', 'day', 'week', 'month'] });
      }
      const ledgerSeq = await currentLedger();
      const wraps = filterByPeriod(await loadWraps(req.params.address), period);
      const limit = parseLimit(req.query.limit, pageSize);
      const cursor = parseCursor(req.query.cursor);
      const page = paginate(wraps, cursor, limit);
      return sendWithEtag(reply, ledgerSeq, {
        address: req.params.address,
        period,
        ledger_seq: ledgerSeq,
        total: wraps.length,
        ...page,
      });
    },
  );

  // ── GET /stats ──────────────────────────────────────────────────────
  app.get('/stats', async (_req, reply) => {
    const ledgerSeq = await currentLedger();
    const stats = await db.getStats();
    return sendWithEtag(reply, ledgerSeq, {
      ledger_seq: ledgerSeq,
      total_wraps: stats.totalWraps,
      total_revoked: stats.totalRevoked,
      archetype_distribution: stats.archetypeDistribution,
      leaderboard: stats.leaderboard,
    });
  });

  // ── GET /health ─────────────────────────────────────────────────────
  app.get('/health', async (_req, reply) => {
    const indexedLedger = await currentLedger();
    let chainHead: number | null = null;
    if (getChainHead) {
      try {
        chainHead = await getChainHead();
      } catch {
        chainHead = null;
      }
    }
    const lag = chainHead === null ? null : Math.max(0, chainHead - indexedLedger);
    const healthy = chainHead === null ? true : lag <= 5;
    return reply.code(healthy ? 200 : 503).send({
      status: healthy ? 'ok' : 'lagging',
      indexed_ledger: indexedLedger,
      chain_head: chainHead,
      lag,
    });
  });

  return app;
}
