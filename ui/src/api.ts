import { transport, type Command } from './transport';
export { native } from './transport';
export type { Command } from './transport';

export function api<T>(command: Command, args?: Record<string, unknown>): Promise<T> {
  return transport.call<T>(command, args);
}
