import { HostEntry } from '../bridge';

export function canPairHost(host: HostEntry) {
  return (host.status === 'Online' || host.status === 'Pairing required') && !host.paired && host.serverSupported;
}

export function canWakeHost(host: HostEntry) {
  return host.status !== 'Online' && host.wakeable;
}
