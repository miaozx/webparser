#!/usr/bin/env python3
"""Eval viewer: ground truth vs extracted markdown. Tables, lists, code, bold etc."""

import http.server, socketserver, json, urllib.parse, os, sys

DATA_FILE = '/tmp/eval100_md.jsonl'

MARKED_PATH = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'marked.min.js')
if not os.path.exists(MARKED_PATH):
    import urllib.request
    print('Downloading marked.min.js...', file=sys.stderr)
    urllib.request.urlretrieve(
        'https://cdn.jsdelivr.net/npm/marked/marked.min.js', MARKED_PATH)
with open(MARKED_PATH) as f:
    MARKED_JS = f.read()

HTML = r'''<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>正文抽取评估 (Markdown)</title>
<script>MARKED_JS</script>
<style>
*{margin:0;padding:0;box-sizing:border-box}
body{background:#0a0a1a;color:#e0e0e0;font-family:-apple-system,'Segoe UI',sans-serif;font-size:14px;line-height:1.6}
.header{background:#16213e;padding:12px 20px;display:flex;align-items:center;gap:16px;flex-wrap:wrap;border-bottom:2px solid #0f3460}
.header h1{font-size:16px;color:#4ecca3}
.stats{font-size:12px;color:#888;margin-left:auto}
.nav{display:flex;align-items:center;gap:8px}
.nav button{background:#0f3460;color:#e0e0e0;border:none;padding:4px 12px;border-radius:4px;cursor:pointer;font-size:14px}
.nav button:hover{background:#1a5276}
.counter{font-size:13px;color:#aaa;min-width:60px;text-align:center}
.main{display:flex;height:calc(100vh - 90px)}
.panel{flex:1;display:flex;flex-direction:column;overflow:hidden}
.panel:first-child{border-right:1px solid #1a1a3e}
.panel-title{background:#16213e;padding:6px 12px;font-size:12px;color:#888;display:flex;align-items:center;gap:8px;flex-wrap:wrap}
.panel-title .url{color:#4ea8de;text-decoration:none;font-size:11px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;max-width:300px}
.panel-content{flex:1;overflow-y:auto;padding:12px 16px;white-space:normal;word-wrap:break-word}
.panel-content p{margin:0 0 8px}
.panel-content table{border-collapse:collapse;margin:8px 0;font-size:13px;width:auto;max-width:100%}
.panel-content th,.panel-content td{border:1px solid #333;padding:4px 8px;text-align:left}
.panel-content th{background:#1a1a3e;font-weight:600}
.panel-content pre{background:#111;border:1px solid #222;border-radius:4px;padding:8px 12px;overflow-x:auto;margin:8px 0}
.panel-content code{background:#111;padding:1px 4px;border-radius:3px;font-size:13px}
.panel-content pre code{background:none;padding:0}
.panel-content blockquote{border-left:3px solid #4ecca3;margin:8px 0;padding:4px 12px;color:#aaa}
.panel-content ul,.panel-content ol{padding-left:20px;margin:4px 0}
.panel-content li{margin:2px 0}
.info-bar{background:#16213e;padding:6px 12px;font-size:11px;color:#666;display:flex;gap:16px;overflow:hidden;white-space:nowrap}
.info-bar span{overflow:hidden;text-overflow:ellipsis}
.tag{display:inline-block;padding:1px 5px;border-radius:3px;font-size:10px;margin:0 2px}
.tag.used{background:#1a4a1a;color:#4ecca3}
.tag.skip{background:#6b1a1a;color:#df8f8f}
.len{color:#888}
.bad{color:#e94560}.good{color:#4ecca3}
.toolbar{display:flex;gap:6px;margin-left:auto}
.view-btn{background:#0f3460;color:#888;border:none;padding:2px 8px;border-radius:3px;cursor:pointer;font-size:11px}
.view-btn.active{color:#4ecca3;background:#1a4a1a}
</style>
</head>
<body>
<div class="header">
  <h1>正文抽取评估 (Markdown)</h1>
  <div class="stats" id="stats"></div>
  <div class="toolbar">
    <button class="view-btn active" onclick="setView('md')" id="btnMd">Markdown</button>
    <button class="view-btn" onclick="setView('text')" id="btnText">纯文本</button>
  </div>
  <div class="nav">
    <button onclick="prev()">&larr;</button>
    <span class="counter" id="counter">0 / 0</span>
    <button onclick="next()">&rarr;</button>
    <input type="number" id="jump" min="1" style="width:50px;background:#0f3460;color:#e0e0e0;border:none;padding:3px 6px;border-radius:3px;font-size:12px;" placeholder="#">
    <button onclick="go()">Go</button>
  </div>
</div>
<div class="main">
  <div class="panel left">
    <div class="panel-title"><span>正确结果</span><a class="url" id="urlLink" target="_blank"></a></div>
    <div class="panel-content" id="leftPanel"></div>
  </div>
  <div class="panel right">
    <div class="panel-title"><span>抽取结果</span><span id="matchInfo" style="font-size:11px"></span></div>
    <div class="panel-content" id="rightPanel"></div>
  </div>
</div>
<div class="info-bar">
  <span id="urlDisplay"></span>
  <span id="lenDisplay"></span>
</div>
<script>
var DATA=[],idx=0,viewMode='md';
function load(){
  fetch('/data').then(function(r){return r.json()}).then(function(d){
    DATA=d;
    document.getElementById('counter').textContent='1 / '+DATA.length;
    render(0);
  });
}
function render(i){
  var d=DATA[i];if(!d)return;
  idx=i;
  document.getElementById('counter').textContent=(i+1)+' / '+DATA.length;
  document.getElementById('leftPanel').innerHTML=marked.parse(d.ground_truth,{breaks:true});
  var right=d.content_markdown||d.extracted;
  if(viewMode==='text') right=d.extracted;
  document.getElementById('rightPanel').innerHTML=marked.parse(right,{breaks:true});
  document.getElementById('urlLink').textContent=d.url;
  document.getElementById('urlLink').href=d.url;
  document.getElementById('urlDisplay').textContent=d.url;
  var r=d.ground_truth_len>0?(d.extracted_len/d.ground_truth_len):0;
  var c=r>1.5?'bad':'good';
  document.getElementById('lenDisplay').innerHTML='原文:<span class="len">'+d.ground_truth_len+'</span> 抽取:<span class="len">'+d.extracted_len+'</span> <span class="'+c+'">x'+r.toFixed(2)+'</span>'+
    ' MD:<span class="len">'+d.md_len+'</span>';
  var pct=d.ground_truth_len>0?Math.min(100,Math.round(d.extracted_len/d.ground_truth_len*100)):0;
  document.getElementById('matchInfo').textContent='抽取/原文: '+pct+'%';
  var ok=0;
  for(var j=0;j<DATA.length;j++){if(DATA[j].extracted_len>0)ok++;}
  document.getElementById('stats').textContent='成功:'+ok+'/'+DATA.length;
}
function setView(m){
  viewMode=m;
  document.getElementById('btnMd').className='view-btn'+(m==='md'?' active':'');
  document.getElementById('btnText').className='view-btn'+(m==='text'?' active':'');
  render(idx);
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
                data = []
                with open(DATA_FILE) as f:
                    for line in f:
                        d = json.loads(line)
                        data.append(d)
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
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 45678
    with socketserver.ThreadingTCPServer(('', port), Handler) as httpd:
        print(f'Eval viewer at http://localhost:{port}', file=sys.stderr)
        httpd.serve_forever()
