/**
 * Capto updater mirror — Cloudflare Worker
 *
 * Proxies the Capto GitHub Release so that in-app update checks and installer
 * downloads can get a faster CDN edge (especially where github.com is slow).
 *
 * Tauri's generated latest.json points `url` at `api.github.com/repos/.../
 * releases/assets/<id>`, which is rate-limited (60/hr/IP, anonymous). We avoid
 * that entirely: the installer filename is fixed by the Release workflow
 * (`Capto_<version>_<arch>-setup.exe`, tag `v<version>`), so we rebuild each
 * `url` from the manifest's `version` into a `github.com/.../releases/download/
 * <tag>/<file>` browser URL — which has no rate limit and redirects to the CDN.
 *
 * Routes:
 *   GET /updates/latest.json       -> stable channel (rolling `updater` tag)
 *   GET /updates/canary.json       -> canary channel (rolling `canary` tag)
 *   GET /updates/download/*        -> stream a GitHub release asset
 *   OPTIONS + CORS preflight, 404 otherwise
 *
 * Channels (progressive rollout): stable users always resolve
 * `latest.json`; testers/beta agents opt into `canary.json`, which reads the
 * separate rolling `canary` release tag. Promote a canary to stable by
 * publishing the same version's latest.json to the `updater` tag (see
 * docs/CI.md).
 */

const GITHUB_REPO = 'ciryanna/Capto';

// The rolling tag that hosts only latest.json (see docs/CI.md).
const UPDATER_TAG = 'updater';
// Rolling canary tag for staged rollout (published by the maintainer).
const CANARY_TAG = 'canary';

const GITHUB_BASE = `https://github.com/${GITHUB_REPO}`;

const DOWNLOAD_ROUTE = '/updates/download/';

// Workerd-compatible cache API (Cloudflare's runtime-provided cache).
const cache = caches.default;

// Tauri updater target -> installer arch token in `Capto_<version>_<arch>-setup.exe`.
const TARGET_ARCH = {
  'windows-x86_64': 'x64',
  'windows-x86_64-nsis': 'x64',
  'windows-aarch64': 'arm64',
  'windows-aarch64-nsis': 'arm64',
};

/**
 * Build the browser download URL for a target from the manifest version.
 * e.g. version "0.4.0", target "windows-x86_64" ->
 *   https://github.com/ciryanna/Capto/releases/download/v0.4.0/Capto_0.4.0_x64-setup.exe
 * Returns null when the target or version can't be mapped.
 */
function browserDownloadUrl(version, target) {
  const ver = String(version || '').trim();
  const arch = TARGET_ARCH[target];
  if (!ver || !arch) return null;
  return `${GITHUB_BASE}/releases/download/v${ver}/Capto_${ver}_${arch}-setup.exe`;
}

/**
 * Map a manifest `url` to this worker's download route. The manifest url is
 * assumed to be an api.github.com asset link for the given target; we discard
 * it in favour of the derived browser URL (so downloads never hit the API).
 */
function rewriteUrl(version, target, requestUrl) {
  const browser = browserDownloadUrl(version, target);
  if (!browser) return null;
  // Encode the upstream URL into the worker path so the download route can
  // restore it verbatim.
  return `${requestUrl.origin}${DOWNLOAD_ROUTE}${encodeURIComponent(browser)}`;
}

function rewriteReleaseJson(json, requestUrl) {
  const out = { ...json };
  const version = out.version;
  if (typeof out.platforms === 'object' && out.platforms !== null) {
    const platforms = {};
    for (const [target, meta] of Object.entries(out.platforms)) {
      const rewritten = meta && meta.url ? rewriteUrl(version, target, requestUrl) : null;
      platforms[target] = { ...meta, url: rewritten ?? (meta && meta.url) };
    }
    out.platforms = platforms;
  } else if (typeof out.url === 'string') {
    // Dynamic shape: no target info. Best-effort: leave as-is (rare).
    out.url = out.url;
  }
  return out;
}

function jsonResponse(body, status) {
  return new Response(body, {
    status,
    headers: {
      'content-type': 'application/json; charset=utf-8',
      'cache-control': 'public, max-age=300, stale-while-revalidate=3600',
      'access-control-allow-origin': '*',
    },
  });
}

async function handleLatest(request, requestUrl, tag) {
  const cacheKey = new Request(`${requestUrl.origin}${requestUrl.pathname}`, request);
  const cached = await cache.match(cacheKey);
  if (cached) return cached;

  const upstreamLatest = `${GITHUB_BASE}/releases/download/${tag}/latest.json`;
  let upstream;
  try {
    upstream = await fetch(upstreamLatest, {
      headers: { 'user-agent': 'capto-update-proxy' },
    });
  } catch (err) {
    return jsonResponse(JSON.stringify({ error: 'upstream_unreachable', message: String(err) }), 502);
  }
  if (!upstream.ok) {
    return jsonResponse(
      JSON.stringify({ error: 'upstream_error', status: upstream.status, statusText: upstream.statusText }),
      upstream.status,
    );
  }

  const text = await upstream.text();
  let parsed;
  try {
    parsed = JSON.parse(text);
  } catch (err) {
    return jsonResponse(JSON.stringify({ error: 'invalid_json', message: String(err) }), 502);
  }

  const rewritten = jsonResponse(JSON.stringify(rewriteReleaseJson(parsed, requestUrl)), 200);
  const upstreamCache = upstream.headers.get('cache-control') || '';
  const maxAge = /max-age=(\d+)/.exec(upstreamCache);
  const age = maxAge ? parseInt(maxAge[1], 10) : 300;
  rewritten.headers.set('cache-control', `public, max-age=${age}, stale-while-revalidate=3600`);
  cache.put(cacheKey, rewritten.clone());
  return rewritten;
}

// Decode the worker-internal download URL back to the upstream GitHub URL.
function upstreamDownloadUrl(requestUrl) {
  const url = new URL(requestUrl);
  const path = url.pathname;
  if (!path.startsWith(DOWNLOAD_ROUTE)) return null;
  const rest = path.slice(DOWNLOAD_ROUTE.length);
  if (!rest) return null;
  try {
    const decoded = decodeURIComponent(rest);
    const parsed = new URL(decoded);
    if (parsed.hostname !== 'github.com') return null;
    return decoded;
  } catch (err) {
    return null;
  }
}

async function handleDownload(request, requestUrl) {
  const upstreamUrl = upstreamDownloadUrl(request.url);
  if (!upstreamUrl) {
    return new Response('Not Found', { status: 404 });
  }

  const cacheKey = new Request(requestUrl, request);
  const cached = await cache.match(cacheKey);
  if (cached) return cached;

  let upstream;
  try {
    // github.com/.../releases/download/<tag>/<file> redirects to the signed CDN
    // asset; no API rate limit. Follow it to stream the binary.
    upstream = await fetch(upstreamUrl, {
      headers: { 'user-agent': 'capto-update-proxy' },
      redirect: 'follow',
    });
  } catch (err) {
    return new Response(`upstream unreachable: ${String(err)}`, { status: 502 });
  }
  if (!upstream.ok) {
    return new Response(
      `upstream error: ${upstream.status} ${upstream.statusText}`,
      { status: upstream.status },
    );
  }
  if (!upstream.body) {
    return new Response('upstream empty body', { status: 502 });
  }

  const headers = new Headers();
  const contentLength = upstream.headers.get('content-length');
  if (contentLength) headers.set('content-length', contentLength);
  const contentType = upstream.headers.get('content-type');
  if (contentType) headers.set('content-type', contentType);
  headers.set('accept-ranges', 'bytes');
  // Cache large installers on the CF edge: 512MB object limit is plenty.
  headers.set('cache-control', 'public, max-age=86400, immutable');
  headers.set('access-control-allow-origin', '*');

  const res = new Response(upstream.body, { status: 200, headers });
  cache.put(cacheKey, res.clone());
  return res;
}

export default {
  async fetch(request, env, ctx) {
    const method = request.method;
    const url = new URL(request.url);

    if (method === 'OPTIONS') {
      return new Response(null, {
        status: 204,
        headers: {
          'access-control-allow-origin': '*',
          'access-control-allow-methods': 'GET, OPTIONS',
          'access-control-allow-headers': '*',
          'access-control-max-age': '86400',
        },
      });
    }
    if (method !== 'GET') {
      return new Response('Method Not Allowed', { status: 405 });
    }

    if (url.pathname.endsWith('/latest.json')) {
      return handleLatest(request, url, UPDATER_TAG);
    }
    if (url.pathname.endsWith('/canary.json')) {
      return handleLatest(request, url, CANARY_TAG);
    }
    if (url.pathname.startsWith(DOWNLOAD_ROUTE)) {
      return handleDownload(request, url);
    }
    return new Response('Not Found', { status: 404 });
  },
};