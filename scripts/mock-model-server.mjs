import http from 'node:http';

const args = new Map(process.argv.slice(2).map((value, index, all) =>
  value.startsWith('--') ? [value.slice(2), all[index + 1]] : [value, value]));
const port = Number(args.get('port') || 0);
const scenario = args.get('scenario') || 'valid';
let calls = 0;
let active = 0;
let maxActive = 0;
let holding = false;
const held = [];
const releaseHeld = () => { holding = false; held.splice(0).forEach(send => send()); };

const annotation = (region, kind = 'concept', text = `Concept for region ${region}.`) =>
  ({ region, kind, title: kind === 'concept' ? `Concept ${region}` : undefined, text });
const regionIds = body => body?.response_format?.json_schema?.schema?.properties
  ?.annotations?.items?.properties?.region?.enum || [1];
const validContent = body => {
  const schema = body?.response_format?.json_schema?.schema;
  if (schema?.properties?.purpose) return JSON.stringify({purpose:'Understand this code.',walkthrough:['Follow the source in order.'],boundaries:['This section alone does not establish wider behavior.'],example:'For this example, follow the shown input through the code.'});
  if (schema?.properties?.explanation) return JSON.stringify({ explanation: 'Detailed teaching for this code.' });
  const regions = regionIds(body);
  const kinds = schema?.properties?.annotations?.items?.properties?.kind?.enum || ['concept'];
  const maximum = schema?.properties?.annotations?.maxItems ?? regions.length;
  return JSON.stringify({
    overview: 'Deterministic concepts for this code.',
    annotations: regions.slice(0, maximum).map(region => annotation(region, kinds[0])),
  });
};
const contentFor = (body, attempt) => {
  const regions = regionIds(body);
  if (scenario === 'malformed_then_valid' && attempt === 0) return 'not json';
  if (scenario === 'always_malformed') return 'not json';
  if (scenario === 'wrong_kind_then_valid' && attempt === 0) {
    return JSON.stringify({ overview: 'Wrong kind.', annotations: [annotation(regions[0], 'change')] });
  }
  if (scenario === 'invalid_region_then_valid' && attempt === 0) {
    return JSON.stringify({ overview: 'Wrong region.', annotations: [annotation(999)] });
  }
  if (scenario === 'missing_coverage_then_valid' && attempt === 0) {
    return JSON.stringify({ overview: 'Incomplete.', annotations: [annotation(regions[0])] });
  }
  if (scenario === 'duplicates') {
    return JSON.stringify({
      overview: 'Duplicate concepts are normalized.',
      annotations: [annotation(regions[0], 'concept', 'Repeated.'), annotation(regions[0], 'concept', 'Repeated.'), annotation(regions[0], 'concept', 'Distinct.')],
    });
  }
  return validContent(body);
};

const server = http.createServer((request, response) => {
  if (request.method === 'GET' && request.url === '/calls') {
    response.setHeader('content-type', 'application/json');
    response.end(JSON.stringify({ calls, active, maxActive, held: held.length }));
    return;
  }
  if (scenario === 'controlled_valid' && request.method === 'POST' && ['/hold', '/release'].includes(request.url)) {
    if (request.url === '/hold') holding = true;
    else releaseHeld();
    response.setHeader('content-type', 'application/json');
    response.end(JSON.stringify({ ok: true }));
    return;
  }
  if (request.method === 'GET' && request.url === '/v1/models') {
    response.setHeader('content-type', 'application/json');
    response.end(JSON.stringify({ data: [{ id: 'mock-model', status: { args: ['mock', '--ctx-size', '16384'] }, meta: { n_ctx_train: 16384 } }] }));
    return;
  }
  if (request.method !== 'POST' || request.url !== '/v1/chat/completions') {
    response.statusCode = 404;
    response.end('not found');
    return;
  }
  let raw = '';
  request.setEncoding('utf8');
  request.on('data', chunk => { raw += chunk; });
  request.on('end', () => {
    const body = JSON.parse(raw);
    const attempt = calls++;
    active++;
    maxActive = Math.max(maxActive, active);
    const send = () => {
      active--;
      response.setHeader('content-type', 'application/json');
      response.end(JSON.stringify({
        choices: [{ finish_reason: 'stop', message: { content: contentFor(body, attempt) } }],
        usage: { prompt_tokens: 100, completion_tokens: 40 },
      }));
    };
    if (holding) held.push(send);
    else if (scenario === 'delayed_valid') setTimeout(send, 750);
    else send();
  });
});

server.listen(port, '127.0.0.1', () => {
  const address = server.address();
  process.stdout.write(`MOCK_MODEL_URL=http://127.0.0.1:${address.port}/v1\n`);
});

for (const signal of ['SIGINT', 'SIGTERM']) {
  process.on(signal, () => { releaseHeld(); server.close(() => process.exit(0)); });
}
