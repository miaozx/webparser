#!/usr/bin/env python3
"""三栏对比：Ground Truth vs 有TA vs 无TA"""

import http.server, socketserver, json, urllib.parse, os, sys

DATA_FILE = '/tmp/ta_comparison.jsonl'

MARKED_PATH = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'marked.min.js')
if not os.path.exists(MARKED_PATH):
    import urllib.request
    print('Downloading marked.min.js...', file=sys.stderr)
    urllib.request.urlretrieve('https://cdn.jsdelivr.net/npm/marked/marked.min.js', MARKED_PATH)
with open(MARKED_PATH) as f:
    MARKED_JS = f.read()

HTML = r'''<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>正文抽取对比: 有TA vs 无TA</title>
<script>MARKED_JS</script>
<style>
*{margin:0;padding:0;box-sizing:border-box}
body{background:#0a0a1a;color:#e0e0e0;font-family:-apple-system,'Segoe UI',sans-serif;font-size:13px;line-height:1.6}
.header{background:#16213e;padding:10px 16px;display:flex;align-items:center;gap:12px;flex-wrap:wrap;border-bottom:2px solid #0f3460}
.header h1{font-size:15px;color:#4ecca3}
.stats{font-size:11px;color:#888;margin-left:auto}
.nav{display:flex;align-items:center;gap:6px}
.nav button{background:#0f3460;color:#e0e0e0;border:none;padding:3px 10px;border-radius:3px;cursor:pointer;font-size:13px}
.nav button:hover{background:#1a5276}
.counter{font-size:12px;color:#aaa;min-width:50px;text-align:center}
.main{display:flex;height:calc(100vh - 80px)}
.panel{flex:1;display:flex;flex-direction:column;overflow:hidden}
.panel+.panel{border-left:1px solid #1a1a3e}
.panel-title{background:#16213e;padding:5px 10px;font-size:11px;color:#888;display:flex;align-items:center;gap:6px;flex-wrap:wrap}
.panel-title .url{color:#4ea8de;text-decoration:none;font-size:10px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;max-width:250px}
.panel-title .tag{display:inline-block;padding:1px 4px;border-radius:2px;font-size:9px}
.panel-title .tag.gt{background:#1a3a1a;color:#4ecca3}
.panel-title .tag.ta{background:#1a2a4a;color:#4ea8de}
.panel-title .tag.nota{background:#4a1a1a;color:#e94560}
.panel-content{flex:1;overflow-y:auto;padding:10px 12px;white-space:normal;word-wrap:break-word}
.panel-content p{margin:0 0 6px}
.panel-content table{border-collapse:collapse;margin:6px 0;font-size:12px;width:auto;max-width:100%}
.panel-content th,.panel-content td{border:1px solid #333;padding:3px 6px;text-align:left}
.panel-content th{background:#1a1a3e}
.panel-content pre{background:#111;border:1px solid #222;border-radius:3px;padding:6px 10px;overflow-x:auto;margin:6px 0;font-size:12px}
.panel-content code{background:#111;padding:1px 3px;border-radius:2px;font-size:12px}
.panel-content pre code{background:none;padding:0}
.panel-content blockquote{border-left:3px solid #4ecca3;margin:6px 0;padding:3px 10px;color:#aaa}
.panel-content ul,.panel-content ol{padding-left:18px;margin:3px 0}
.info-bar{background:#16213e;padding:4px 10px;font-size:10px;color:#666;display:flex;gap:10px;overflow:hidden;white-space:nowrap}
.info-bar span{overflow:hidden;text-overflow:ellipsis}
.len{color:#888}.bad{color:#e94560}.good{color:#4ecca3}
</style>
</head>
<body>
<div class="header">
  <h1>正文抽取对比</h1>
  <div class="stats" id="stats"></div>
  <div class="nav">
    <button onclick="prev()">&larr;</button>
    <span class="counter" id="counter">0 / 0</span>
    <button onclick="next()">&rarr;</button>
    <input type="number" id="jump" min="1" style="width:45px;background:#0f3460;color:#e0e0e0;border:none;padding:2px 5px;border-radius:2px;font-size:11px;" placeholder="#">
    <button onclick="go()">Go</button>
  </div>
</div>
<div class="main">
  <div class="panel">
    <div class="panel-title"><span class="tag gt">GT</span> Ground Truth <a class="url" id="urlLink" target="_blank"></a></div>
    <div class="panel-content" id="leftPanel"></div>
  </div>
  <div class="panel">
    <div class="panel-title"><span class="tag ta">TA</span> 有title_anchored <span id="infoTA" style="font-size:10px;color:#666"></span></div>
    <div class="panel-content" id="midPanel"></div>
  </div>
  <div class="panel">
    <div class="panel-title"><span class="tag nota">noTA</span> 无title_anchored <span id="infoNoTA" style="font-size:10px;color:#666"></span></div>
    <div class="panel-content" id="rightPanel"></div>
  </div>
</div>
<div class="info-bar">
  <span id="urlDisplay"></span>
  <span id="lenDisplay"></span>
</div>
<script>
var DATA=[],idx=0;
function load(){
  fetch('/data').then(function(r){return r.json()}).then(function(d){
    DATA=d;
    // map field names
    DATA.forEach(function(rec){
      rec.extracted_ta = rec.ta_on_md || '';
      rec.extracted_nota = rec.ta_off_md || '';
      rec.extracted_len_ta = (rec.ta_on_md || '').length;
      rec.extracted_len_nota = (rec.ta_off_md || '').length;
      rec.md_len_ta = (rec.ta_on_md || '').length;
      rec.md_len_nota = (rec.ta_off_md || '').length;
    });
    document.getElementById('counter').textContent='1 / '+DATA.length;
    render(0);
  });
}
function render(i){
  var d=DATA[i];if(!d)return;
  idx=i;
  document.getElementById('counter').textContent=(i+1)+' / '+DATA.length;
  document.getElementById('leftPanel').innerHTML=marked.parse(d.ground_truth,{breaks:true});
  document.getElementById('midPanel').innerHTML=marked.parse(d.extracted_ta||'[empty]',{breaks:true});
  document.getElementById('rightPanel').innerHTML=marked.parse(d.extracted_nota||'[empty]',{breaks:true});
  document.getElementById('urlLink').textContent=d.url;
  document.getElementById('urlLink').href=d.url;
  document.getElementById('urlDisplay').textContent=d.url;
  var same=d.extracted_len_ta==d.extracted_len_nota;
  var sameMd=d.md_len_ta==d.md_len_nota;
  document.getElementById('infoTA').textContent=d.extracted_len_ta+'chars'+(same?' (same)':'');
  document.getElementById('infoNoTA').textContent=d.extracted_len_nota+'chars';
  document.getElementById('stats').textContent='TA='+d.md_len_ta+'md noTA='+d.md_len_nota+'md';
  document.getElementById('lenDisplay').innerHTML=
    'GT:<span class="len">'+d.ground_truth_len+'</span>'+
    ' TA:<span class="len">'+d.extracted_len_ta+'</span>'+
    ' noTA:<span class="len">'+d.extracted_len_nota+'</span>';
}
function next(){render(Math.min(idx+1,DATA.length-1))}
function prev(){render(Math.max(idx-1,0))}
function go(){var n=parseInt(document.getElementById('jump').value);if(n>=1&&n<=DATA.length)render(n-1)}
document.addEventListener('keydown',function(e){
  if(e.key==='ArrowRight'||e.key===' '){e.preventDefault();next()}
  if(e.key==='ArrowLeft'){e.preventDefault();prev()}
});
</script>
<script>load()</script>
</body>
</html>'''

class Handler(http.server.SimpleHTTPRequestHandler):
    def do_GET(self):
        try:
            p = urllib.parse.urlparse(self.path).path
            if p == '/data':
                data = [json.loads(l) for l in open(DATA_FILE)]
                self.send_response(200)
                self.send_header('Content-Type', 'application/json')
                self.end_headers()
                self.wfile.write(json.dumps(data, ensure_ascii=False).encode())
            elif p == '/':
                self.send_response(200)
                self.send_header('Content-Type', 'text/html; charset=utf-8')
                self.end_headers()
                self.wfile.write(HTML.replace('MARKED_JS', MARKED_JS).encode('utf-8'))
            else:
                super().do_GET()
        except BrokenPipeError:
            pass

if __name__ == '__main__':
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 45679
    print(f'Compare viewer at http://localhost:{port}', file=sys.stderr)
    with socketserver.ThreadingTCPServer(('', port), Handler) as httpd:
        httpd.serve_forever()
