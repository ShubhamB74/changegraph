export interface Session {
  userId: string;
  expiresAt: number;
}

export function createSession(userId: string, ttlMs: number): Session {
  return { userId, expiresAt: Date.now() + ttlMs };
}

export function isExpired(session: Session, now = Date.now()): boolean {
  return session.expiresAt <= now;
}
