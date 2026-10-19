// Applied to the renderer subprocess: HTTP is restricted to its local composition server.
import http from 'node:http';
import https from 'node:https';
import {syncBuiltinESMExports} from 'node:module';

function allowed(value) {
  const url = typeof value === 'string' || value instanceof URL ? new URL(value) : value;
  const host = url.hostname ?? url.host;
  if (!['localhost','127.0.0.1','::1','[::1]'].includes(host)) throw new Error('README rendering permits only loopback HTTP.');
}
const originalFetch = globalThis.fetch;
globalThis.fetch = (url, options) => { allowed(url); return originalFetch(url, options); };
for (const module of [http,https]) for (const name of ['request','get']) {
  const original = module[name];
  module[name] = function (url,...args) { allowed(url); return original.call(this,url,...args); };
}
syncBuiltinESMExports();
