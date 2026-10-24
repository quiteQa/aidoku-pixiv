// Runs the actual release WASM through Aidoku's ABI with deterministic HTTP mocks.
// No account credentials or network access are used by this test suite.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';

const module = new WebAssembly.Module(readFileSync(new URL('../target/wasm32-unknown-unknown/release/aidoku_pixiv.wasm', import.meta.url)));
const bytes = s => [...Buffer.from(s)];
const uint = n => { const result = []; do { const b = n % 128; n = Math.floor(n / 128); result.push(b | (n ? 128 : 0)); } while (n); return result; };
const string = s => [...uint(Buffer.byteLength(s)), ...bytes(s)];
const array = (values, encoder) => [...uint(values.length), ...values.flatMap(encoder)];
const option = (value, encoder) => value == null ? [0] : [1, ...encoder(value)];

class Reader {
  constructor(data) { this.data = Buffer.from(data); this.index = 0; }
  byte() { assert.ok(this.index < this.data.length, 'unexpected end of postcard'); return this.data[this.index++]; }
  uint() { let n = 0, scale = 1, b; do { b = this.byte(); n += (b & 127) * scale; scale *= 128; } while (b & 128); return n; }
  string() { const len = this.uint(); const result = this.data.subarray(this.index, this.index + len).toString(); this.index += len; return result; }
  option(read) { return this.byte() ? read.call(this) : null; }
  array(read) { return Array.from({ length: this.uint() }, () => read.call(this)); }
  float() { const n = this.data.readFloatLE(this.index); this.index += 4; return n; }
  chapter() { return { key: this.string(), title: this.option(this.string), number: this.option(this.float), volume: this.option(this.float), date: this.option(this.uint), groups: this.option(() => this.array(this.string)), url: this.option(this.string), lang: this.option(this.string), thumbnail: this.option(this.string), locked: this.byte() }; }
  manga() { return { key: this.string(), title: this.string(), cover: this.option(this.string), artists: this.option(() => this.array(this.string)), authors: this.option(() => this.array(this.string)), description: this.option(this.string), url: this.option(this.string), tags: this.option(() => this.array(this.string)), status: this.uint(), rating: this.uint(), viewer: this.uint(), strategy: this.uint(), nextUpdate: this.option(this.uint), chapters: this.option(() => this.array(this.chapter)) }; }
  list() { return { entries: this.array(this.manga), next: !!this.byte() }; }
  pages() { return this.array(function () { assert.equal(this.uint(), 0); const url = this.string(); assert.equal(this.byte(), 0); this.option(this.string); this.byte(); this.option(this.string); return url; }); }
}

async function host(initial = {}, responder = () => { throw Error('unexpected HTTP request'); }) {
  const prefs = new Map(Object.entries(initial)), objects = new Map(), requests = [], logs = [];
  let nextId = 1, api, now = 1_792_800_000;
  const save = value => { const id = nextId++; objects.set(id, value); return id; };
  const memory = () => new Uint8Array(api.memory.buffer);
  const read = (p, n) => Buffer.from(memory().slice(p, p + n)).toString();
  const copy = (p, data) => { memory().set(data, p); return 0; };
  const result = ptr => {
    assert.ok(ptr > 0, `WASM returned error code ${ptr}`);
    const view = new DataView(api.memory.buffer), len = view.getInt32(ptr, true);
    if (len === -1) {
      const realLen = view.getInt32(ptr + 8, true);
      const error = read(ptr + 12, realLen - 12); api.free_result(ptr); throw Error(error);
    }
    const data = Buffer.from(memory().slice(ptr + 8, ptr + len)); api.free_result(ptr); return new Reader(data);
  };
  const imports = {
    env: { print: (p, n) => logs.push(read(p, n)), abort: () => { throw Error('WASM aborted'); } },
    std: {
      destroy: id => objects.delete(id), current_date: () => now,
      buffer_len: id => objects.get(id)?.length ?? -1,
      read_buffer: (id, p) => copy(p, objects.get(id)),
    },
    defaults: {
      get: (p, n) => {
        const key = read(p, n); if (!prefs.has(key)) return -1;
        const value = prefs.get(key);
        return save(Buffer.from(typeof value === 'string' ? string(value) : Array.isArray(value) ? array(value, string) : [Number(value)]));
      },
      set: (p, n, kind, ptr) => {
        const key = read(p, n); if (kind === 6) { prefs.delete(key); return 0; }
        const view = new DataView(api.memory.buffer), len = view.getInt32(ptr, true);
        const r = new Reader(memory().slice(ptr + 8, ptr + len));
        prefs.set(key, kind === 4 ? r.string() : kind === 5 ? r.array(r.string) : !!r.byte()); return 0;
      },
    },
    net: {
      init: method => save({ method, headers: {} }),
      set_url: (id, p, n) => { objects.get(id).url = read(p, n); return 0; },
      set_header: (id, kp, kn, vp, vn) => { objects.get(id).headers[read(kp, kn).toLowerCase()] = read(vp, vn); return 0; },
      set_body: (id, p, n) => { objects.get(id).body = read(p, n); return 0; },
      set_timeout: () => 0, set_rate_limit: () => 0,
      send: id => { const request = objects.get(id); requests.push(structuredClone(request)); const response = responder(request, requests); request.status = response.status ?? 200; request.data = Buffer.from(typeof response.body === 'string' ? response.body : JSON.stringify(response.body)); return 0; },
      get_status_code: id => objects.get(id).status,
      data_len: id => objects.get(id).data.length,
      read_data: (id, p) => copy(p, objects.get(id).data),
    },
    html: {
      parse: (p, n) => save(read(p, n)), select_first: id => save(objects.get(id)),
      text: id => save(Buffer.from(objects.get(id).replace(/<[^>]*>/g, '').replace(/&amp;/g, '&'))),
    },
  };
  const instance = await WebAssembly.instantiate(module, imports); api = instance.exports; api.start();
  const descriptor = data => save(Buffer.from(data));
  return {
    prefs, requests, logs, tick: seconds => { now += seconds; },
    restart: () => api.start(),
    login: cookies => result(api.handle_web_login(descriptor(string('webLogin')), descriptor(array(Object.keys(cookies), string)), descriptor(array(Object.values(cookies), string)))).byte(),
    notify: name => assert.equal(api.handle_notification(descriptor(string(name))), 0),
    search: (query = '', page = 1, filters = []) => result(api.get_search_manga_list(descriptor(bytes(query)), page, descriptor([ ...uint(filters.length), ...filters.flat() ]))).list(),
    images: url => {
      const rid = result(api.get_image_request(descriptor(string(url)), -1)).uint();
      // i32 uses zigzag encoding in postcard; a positive Rid is doubled.
      return objects.get(rid / 2);
    },
    detail: (key, details = true, chapters = true) => {
      const input = [...string(key), ...string(''), ...Array(6).fill(0), ...Array(4).fill(0), 0, 0];
      return result(api.get_manga_update(descriptor(input), details, chapters)).manga();
    },
    pages: key => {
      const manga = [...string(key), ...string(''), ...Array(6).fill(0), ...Array(4).fill(0), 0, 0];
      const chapter = [...string(key), ...Array(8).fill(0), 0];
      return result(api.get_page_list(descriptor(manga), descriptor(chapter))).pages();
    },
  };
}

const appItem = { id: 123, title: '测试作品', type: 'manga', user: { name: '作者' }, image_urls: { large: 'https://i.pximg.net/cover.jpg' }, page_count: 2, meta_pages: [{ image_urls: { original: 'https://i.pximg.net/0.png', large: 'https://i.pximg.net/0.jpg' } }, { image_urls: { original: 'https://i.pximg.net/1.png', large: 'https://i.pximg.net/1.jpg' } }], tags: [{ name: '猫' }], x_restrict: 0 };
const appList = { illusts: [appItem], next_url: null };
let passed = 0;
async function test(name, run) { await run(); passed++; console.log(`PASS ${name}`); }

await test('anonymous cookies do not complete web login', async () => {
  const h = await host(); assert.equal(h.login({ device_token: 'anonymous' }), 0); assert.equal(h.requests.length, 0);
  assert.throws(() => h.search(), /网页登录/);
});
await test('web login validates session and selects web mode', async () => {
  const h = await host({ authMode: 'token' }, r => { assert.equal(r.headers.cookie, 'PHPSESSID=123_session'); assert.ok(!r.headers.authorization); return { body: { error: false, body: { following: 0 } } }; });
  assert.equal(h.login({ PHPSESSID: '123_session' }), 1); assert.equal(h.prefs.get('authMode'), 'web');
});
await test('stale session is rejected', async () => {
  const h = await host({}, () => ({ status: 400, body: { error: true } })); assert.equal(h.login({ PHPSESSID: 'stale' }), 0);
});
await test('web search encodes query, filters and preserves upstream pagination', async () => {
  const h = await host({ webLogin: 'logged_in', 'webLogin.keys': ['PHPSESSID'], 'webLogin.values': ['123_session'] }, r => {
    const u = new URL(r.url); assert.equal(u.searchParams.get('word'), '猫 &+'); assert.equal(u.searchParams.get('s_mode'), 's_tag_full');
    assert.equal(u.searchParams.get('order'), 'date'); assert.equal(u.searchParams.get('p'), '2');
    return { body: { error: false, body: { illustManga: { data: [{ id: '123', title: '猫', illustType: 0, xRestrict: 0, aiType: 1 }], total: 121 } } } };
  });
  const list = h.search('猫 &+', 2, [[3, ...string('match'), ...string('exact')], [1, ...string('sort'), 0, 1]]);
  assert.equal(list.entries[0].key, '123'); assert.equal(list.next, true);
});
await test('access token has no refresh requests or cookie leakage', async () => {
  const h = await host({ authMode: 'token', tokenKind: 'access', tokenInput: ' Bearer direct_token ' }, r => {
    assert.ok(r.url.startsWith('https://app-api.pixiv.net/')); assert.equal(r.headers.authorization, 'Bearer direct_token'); assert.ok(!r.headers.cookie); return { body: appList };
  });
  assert.equal(h.search('猫').entries.length, 1); assert.equal(h.requests.length, 1);
  const image = h.images('https://i.pximg.net/0.png'); assert.equal(image.headers.referer, 'https://www.pixiv.net/'); assert.ok(!image.headers.authorization); assert.ok(!image.headers.cookie);
});
await test('refresh token exchange, client hash, cache and rotation', async () => {
  let exchanges = 0;
  const h = await host({ authMode: 'token', tokenInput: 'first_refresh' }, r => {
    if (r.url.includes('oauth.secure')) {
      const form = new URLSearchParams(r.body); assert.equal(form.get('grant_type'), 'refresh_token');
      assert.equal(form.get('refresh_token'), exchanges ? 'rotated_refresh' : 'first_refresh'); exchanges++;
      const expected = createHash('md5').update(r.headers['x-client-time'] + '28c1fdd170a5204386cb1313c7077b34f83e4aaf4aa829ce78c231e05b0bae2c').digest('hex');
      assert.equal(r.headers['x-client-hash'], expected); assert.ok(!r.headers.authorization);
      return { body: { response: { access_token: `access_${exchanges}`, refresh_token: 'rotated_refresh', expires_in: 3600 } } };
    }
    assert.equal(r.headers.authorization, `Bearer access_${exchanges}`); return { body: appList };
  });
  h.search('a'); h.search('a'); assert.equal(exchanges, 1); h.tick(3600); h.search('a'); assert.equal(exchanges, 2);
  h.prefs.set('tokenInput', 'new_account'); h.notify('tokenChanged'); assert.ok(!h.prefs.has('cachedAccess')); assert.ok(!h.prefs.has('cachedRefresh'));
});
await test('401 retries refresh exactly once, then returns an error', async () => {
  let exchanges = 0;
  const h = await host({ authMode: 'token', tokenInput: 'refresh' }, r => r.url.includes('oauth.secure')
    ? { body: { access_token: `access_${++exchanges}`, expires_in: 3600 } }
    : { status: 401, body: {} });
  assert.throws(() => h.search('a'), /Access Token/); assert.equal(exchanges, 2); assert.equal(h.requests.length, 4);
});
await test('invalid refresh and rate limit surface actionable errors', async () => {
  const invalid = await host({ authMode: 'token', tokenInput: 'bad_refresh' }, () => ({ status: 400, body: { access_token: 'must_not_leak' } }));
  assert.throws(() => invalid.search(), /Refresh Token/); assert.ok(!invalid.logs.join('').includes('bad_refresh'));
  const limited = await host({ authMode: 'token', tokenKind: 'access', tokenInput: 'access' }, () => ({ status: 429, body: {} }));
  assert.throws(() => limited.search(), /频繁/); assert.equal(limited.requests.length, 1);
});
await test('actual WASM details, chapters and multi-page image order', async () => {
  const h = await host({ authMode: 'token', tokenKind: 'access', tokenInput: 'access' }, () => ({ body: { illust: appItem } }));
  const detail = h.detail('123'); assert.equal(detail.title, '测试作品'); assert.equal(detail.chapters[0].key, '123');
  assert.deepEqual(h.pages('123'), ['https://i.pximg.net/0.png', 'https://i.pximg.net/1.png']);
  h.prefs.set('originalImages', false); assert.deepEqual(h.pages('123'), ['https://i.pximg.net/0.jpg', 'https://i.pximg.net/1.jpg']);
});
await test('web ranking, details, pages and logged-out session isolation', async () => {
  const webItem = { id: '123', title: '网页作品', userName: '作者', pageCount: 2, aiType: 1, xRestrict: 0, urls: { regular: 'https://i.pximg.net/cover.jpg' }, tags: { tags: [{ tag: '猫' }] } };
  const h = await host({ webLogin: 'logged_in', 'webLogin.keys': ['PHPSESSID'], 'webLogin.values': ['123_session'] }, r => {
    assert.equal(r.headers.cookie, 'PHPSESSID=123_session');
    if (r.url.includes('ranking.php')) return { body: { contents: [{ illust_id: 123, title: '排行作品', illust_type: '1' }], next: 2 } };
    if (r.url.includes('/recommend/illusts?')) return { body: { error: false, body: { illusts: [webItem] } } };
    if (r.url.includes('/pages?')) return { body: { error: false, body: [{ urls: { original: 'https://i.pximg.net/0.png' } }, { urls: { original: 'https://i.pximg.net/1.png' } }] } };
    return { body: { error: false, body: webItem } };
  });
  assert.equal(h.search().entries[0].title, '排行作品'); assert.equal(h.search().next, true);
  assert.equal(h.detail('123').chapters[0].key, '123'); assert.deepEqual(h.pages('123'), ['https://i.pximg.net/0.png', 'https://i.pximg.net/1.png']);
  for (const key of ['webLogin','webLogin.keys','webLogin.values']) h.prefs.delete(key);
  const count = h.requests.length; assert.throws(() => h.search(), /网页登录/); assert.equal(h.requests.length, count);
});
await test('HTTP 400 OAuth error renews token and resumes the original search', async () => {
  let exchanges = 0, apiCalls = 0;
  const h = await host({ authMode: 'token', tokenInput: 'refresh' }, r => {
    if (r.url.includes('oauth.secure')) return { body: { access_token: `access_${++exchanges}`, expires_in: 3600 } };
    apiCalls++; return apiCalls === 1 ? { status: 400, body: { error: { message: 'Error occurred at the OAuth process.' } } } : { body: appList };
  });
  assert.equal(h.search('a').entries[0].key, '123'); assert.equal(exchanges, 2); assert.equal(apiCalls, 2);
});
await test('access token expiration never attempts refresh or web fallback', async () => {
  const h = await host({ authMode: 'token', tokenKind: 'access', tokenInput: 'expired', webLogin: 'logged_in' }, r => {
    assert.ok(r.url.startsWith('https://app-api.pixiv.net/')); return { status: 401, body: {} };
  });
  assert.throws(() => h.search(), /Access Token/); assert.equal(h.requests.length, 1);
});
await test('validation status is visible and does not mark invalid tokens as logged in', async () => {
  const ok = await host({ authMode: 'token', tokenKind: 'access', tokenInput: 'access' }, r => {
    assert.ok(r.url.includes('/v1/illust/follow?')); return { body: appList };
  });
  ok.notify('validateLogin'); assert.match(ok.prefs.get('loginStatus'), /验证成功/);
  const bad = await host({ authMode: 'token', tokenKind: 'access', tokenInput: 'invalid' }, () => ({ status: 401, body: {} }));
  bad.notify('validateLogin'); assert.match(bad.prefs.get('loginStatus'), /已失效/);
});
await test('clear token removes credentials and blocks subsequent API access', async () => {
  const h = await host({ authMode: 'token', tokenInput: 'refresh', cachedAccess: 'cached', cachedRefresh: 'rotated', expiresAt: '9999999999', tokenFingerprint: 'hash' });
  h.notify('clearToken'); for (const key of ['tokenInput', 'cachedAccess', 'cachedRefresh', 'expiresAt', 'tokenFingerprint']) assert.ok(!h.prefs.has(key));
  assert.throws(() => h.search(), /填写 Token/); assert.equal(h.requests.length, 0);
});
await test('content switches independently filter app search and preserve pagination', async () => {
  const entries = [
    { ...appItem, id: 1, illust_ai_type: 1, x_restrict: 0 },
    { ...appItem, id: 2, illust_ai_type: 2, x_restrict: 0 },
    { ...appItem, id: 3, illust_ai_type: 1, x_restrict: 1 },
    { ...appItem, id: 4, illust_ai_type: 2, x_restrict: 2 },
  ];
  for (const [showAI, showAdult, ids] of [[false,false,['1']], [true,false,['1','2']], [false,true,['1','3']], [true,true,['1','2','3','4']]]) {
    const h = await host({ authMode:'token', tokenKind:'access', tokenInput:'access', showAI, showAdult }, r => {
      assert.equal(new URL(r.url).searchParams.get('search_ai_type'), showAI ? '1' : '0');
      return { body: { illusts: entries, next_url:'https://app-api.pixiv.net/next' } };
    });
    const page = h.search('猫'); assert.deepEqual(page.entries.map(m => m.key), ids); assert.equal(page.next, true);
  }
  const h = await host({ authMode:'token', tokenKind:'access', tokenInput:'access' }, () => ({ body: { illusts:entries.slice(1), next_url:'next' } }));
  const page = h.search('猫'); assert.equal(page.entries.length, 0); assert.equal(page.next, true);
});
await test('web search uses server flags and local filtering in both switch states', async () => {
  const entries = [{id:'1',aiType:1,xRestrict:0},{id:'2',aiType:2,xRestrict:0},{id:'3',aiType:1,xRestrict:1}];
  for (const [showAI, showAdult, ids] of [[false,false,['1']], [true,true,['1','2','3']]]) {
    const h = await host({ webLogin:'logged_in', 'webLogin.keys':['PHPSESSID'], 'webLogin.values':['123_session'], showAI, showAdult }, r => {
      const params = new URL(r.url).searchParams; assert.equal(params.get('ai_type'), showAI ? '0' : '1'); assert.equal(params.get('mode'), showAdult ? 'all' : 'safe');
      return { body: { error:false, body:{illustManga:{data:entries,total:120}} } };
    });
    const page = h.search('猫'); assert.deepEqual(page.entries.map(m => m.key), ids); assert.equal(page.next, true); assert.equal(h.requests.length, 1);
  }
});
await test('ranking batches missing AI metadata without changing rank order', async () => {
  const h = await host({ webLogin:'logged_in', 'webLogin.keys':['PHPSESSID'], 'webLogin.values':['123_session'] }, r => {
    if (r.url.includes('ranking.php')) return {body:{contents:[{illust_id:3,title:'third'},{illust_id:2,title:'AI'},{illust_id:1,title:'first'}],next:2}};
    const ids = new URL(r.url).searchParams.getAll('illust_ids[]'); assert.deepEqual(ids,['3','2','1']);
    return {body:{error:false,body:{illusts:[{id:'1',aiType:1,xRestrict:0},{id:'2',aiType:2,xRestrict:0},{id:'3',aiType:1,xRestrict:0}]}}};
  });
  const page=h.search(); assert.deepEqual(page.entries.map(m=>m.key),['3','1']); assert.equal(page.next,true); assert.equal(h.requests.length,2);
});
await test('filters also protect direct IDs and already saved reader entries', async () => {
  for (const field of ['showAI','showAdult']) {
    const item = { ...appItem, illust_ai_type:field==='showAI'?2:1, x_restrict:field==='showAdult'?1:0 };
    const h = await host({authMode:'token',tokenKind:'access',tokenInput:'access'},()=>({body:{illust:item}}));
    assert.throws(()=>h.search('123'),/内容过滤/); assert.throws(()=>h.pages('123'),/内容过滤/);
    h.prefs.set(field,true); assert.equal(h.search('123').entries[0].key,'123'); assert.equal(h.pages('123').length,2);
  }
});
await test('old adult preference migrates once and fresh installs default to web login', async () => {
  const old = await host({hideAdult:false}); assert.equal(old.prefs.get('showAdult'),true);
  old.prefs.set('showAdult',false); old.restart(); assert.equal(old.prefs.get('showAdult'),false);
  const hidden = await host({hideAdult:true}); assert.equal(hidden.prefs.get('showAdult'),false);
  const fresh = await host(); assert.equal(fresh.prefs.get('authMode'),'web'); assert.equal(fresh.prefs.get('showAI')??false,false); assert.equal(fresh.prefs.get('showAdult')??false,false);
});
const settings = JSON.parse(readFileSync(new URL('../sources/multi.pixiv/res/settings.json', import.meta.url), 'utf8'));
const webLoginSetting = settings.flatMap(group => group.items).find(item => item.key === 'webLogin');
const webCookies = { PHPSESSID:'123_session', cc1:'JP', device_token:'device' };
const storedCookies = h => Object.fromEntries((h.prefs.get('webLogin.keys')??[]).map((key,i)=>[key,h.prefs.get('webLogin.values')[i]]));
const webResponse = request => request.url.includes('/ajax/user/extra?')
  ? {body:{error:false,body:{following:1}}}
  : {body:{contents:[{illust_id:123,title:'saved session',aiType:1,xRestrict:0}],next:0}};
await test('native login captures Pixiv parent-domain session cookies', async () => {
  const jar = [{name:'PHPSESSID',value:'123_session',domain:'.pixiv.net'}, {name:'cc1',value:'JP',domain:'.pixiv.net'}];
  // Match Aidoku's WKWebView.getCookies(for:) host filtering exactly.
  const domain = new URL(webLoginSetting.url).hostname;
  const captured = Object.fromEntries(jar.filter(cookie=>cookie.domain.includes(domain)).map(cookie=>[cookie.name,cookie.value]));
  assert.equal(captured.PHPSESSID,'123_session','login entry host must include the .pixiv.net cookie domain');
  const h = await host({},webResponse); assert.equal(h.login(captured),1);
});
await test('callback persists complete web login before any host UI commit', async () => {
  const h = await host({},r=>{
    const cookies=Object.fromEntries(r.headers.cookie.split('; ').map(pair=>pair.split('=')));
    assert.equal(cookies.PHPSESSID,'123_session'); assert.equal(cookies.cc1,'JP'); assert.equal(cookies.device_token,'device');
    assert.ok(!r.headers['user-agent'],'web requests should use Aidoku default UA');
    return webResponse(r);
  });
  assert.equal(h.login(webCookies),1); assert.deepEqual(storedCookies(h),webCookies); assert.equal(h.prefs.get('webLogin'),'logged_in');
  h.restart(); assert.equal(h.search().entries[0].key,'123');
});
await test('existing cookies recover a missing v1/v2 logged-in marker', async () => {
  const h=await host({'webLogin.keys':Object.keys(webCookies),'webLogin.values':Object.values(webCookies)},webResponse);
  assert.equal(h.search().entries[0].key,'123'); assert.equal(h.prefs.get('webLogin'),'logged_in');
});
await test('transient verification failure preserves cookies for a later retry', async () => {
  let blocked=true;
  const h=await host({},r=>blocked ? {status:503,body:{}} : webResponse(r));
  assert.throws(()=>h.login(webCookies),/验证|HTTP/); assert.deepEqual(storedCookies(h),webCookies); assert.ok(!h.prefs.get('webLogin'));
  blocked=false; assert.equal(h.search().entries[0].key,'123'); assert.equal(h.prefs.get('webLogin'),'logged_in');
});
await test('logout and cleared browser cookies cannot restore a stale session', async () => {
  const h=await host({},webResponse); assert.equal(h.login(webCookies),1);
  for(const key of ['webLogin','webLogin.keys','webLogin.values']) h.prefs.delete(key);
  h.notify('webLoginChanged'); h.restart(); const before=h.requests.length;
  assert.throws(()=>h.search(),/登录/); assert.equal(h.requests.length,before);
  assert.equal(h.login(webCookies),1); assert.equal(h.login({device_token:'guest'}),0);
  assert.ok(!h.prefs.get('webLogin')); assert.deepEqual(storedCookies(h),{});
});
await test('site cookies are sent safely and Cloudflare cookies stay host-managed', async () => {
  const cookies={...webCookies,cf_clearance:'old',__cf_bm:'old','bad;name':'bad',injected:'bad\r\nX-Injected:yes'};
  const h=await host({},r=>{
    assert.ok(r.headers.cookie.includes('cc1=JP')); assert.ok(!r.headers.cookie.includes('cf_clearance'));
    assert.ok(!r.headers.cookie.includes('__cf')); assert.ok(!r.headers.cookie.includes('bad;name')); assert.ok(!r.headers.cookie.includes('\r'));
    return webResponse(r);
  });
  assert.equal(h.login(cookies),1); h.search(); assert.ok(!h.logs.join('').includes('123_session'));
});
console.log(`${passed} WASM integration tests passed.`);
