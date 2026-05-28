#!/usr/bin/env python3
"""一键启动评估工具：左右对比正确结果 vs 抽取结果，← → 空格翻页"""

import http.server, socketserver, json, urllib.parse, os, sys

DATA_FILE = '/tmp/eval100_full.jsonl'

# embed marked.min.js
MARKED_PATH = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'marked.min.js')
if not os.path.exists(MARKED_PATH):
    print('Downloading marked.min.js...')
    import urllib.request
    urllib.request.urlretrieve(
        'https://cdn.jsdelivr.net/npm/marked/marked.min.js', MARKED_PATH)
with open(MARKED_PATH) as f:
    MARKED_JS = f.read()

with open(DATA_FILE) as f:
    ENTRIES = [json.loads(line) for line in f]

HTML = r'''<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>正文抽取评估</title>
<script>MARKED_JS</script>
<style>
*{margin:0;padding:0;box-sizing:border-box}
body{font-family:-apple-system,'Microsoft YaHei',sans-serif;background:#1a1a2e;color:#e0e0e0;height:100vh;display:flex;flex-direction:column}
.header{background:#16213e;padding:10px 20px;display:flex;align-items:center;gap:16px;border-bottom:1px solid #0f3460;flex-shrink:0;flex-wrap:wrap}
.header h1{font-size:15px;color:#e94560}
.nav{display:flex;align-items:center;gap:8px;margin-left:auto}
.nav button{background:#0f3460;color:#e0e0e0;border:none;padding:5px 14px;border-radius:4px;cursor:pointer;font-size:13px}
.nav button:hover{background:#e94560}
.counter{font-size:13px;color:#888;min-width:80px;text-align:center}
.stats{font-size:12px;color:#666}
.main{display:flex;flex:1;min-height:0}
.panel{flex:1;display:flex;flex-direction:column;min-width:0}
.panel.left{border-right:1px solid #0f3460}
.panel-title{padding:6px 12px;font-size:12px;color:#aaa;background:#16213e;border-bottom:1px solid #0f3460;flex-shrink:0;display:flex;align-items:center;gap:12px;flex-wrap:wrap}
.panel-title .url{color:#4a8fd4;text-decoration:none;font-size:11px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;max-width:400px}
.panel-content{flex:1;overflow:auto;padding:16px 24px}
.panel-content h1,.panel-content h2,.panel-content h3,.panel-content h4{color:#e94560;margin:12px 0 6px}
.panel-content h1{font-size:20px}.panel-content h2{font-size:17px}.panel-content h3{font-size:15px}
.panel-content p{margin:6px 0;line-height:1.7}
.panel-content a{color:#4a8fd4}
.panel-content blockquote{border-left:3px solid #e94560;padding-left:12px;margin:6px 0;color:#aaa}
.panel-content code{background:#0f3460;padding:1px 5px;border-radius:3px;font-size:12px}
.panel-content pre{background:#0f3460;padding:12px;border-radius:4px;overflow-x:auto}
.panel-content pre code{padding:0;background:none}
.panel-content hr{border:none;border-top:1px solid #333;margin:12px 0}
.panel-content table{border-collapse:collapse;margin:6px 0}
.panel-content td,.panel-content th{border:1px solid #444;padding:5px 10px}
.panel-content ul,.panel-content ol{padding-left:24px;margin:6px 0}
.panel-content li{margin:3px 0;line-height:1.6}
.info-bar{background:#16213e;padding:6px 20px;display:flex;gap:20px;font-size:12px;color:#aaa;border-top:1px solid #0f3460;flex-shrink:0;align-items:center;flex-wrap:wrap}
.tag{display:inline-block;padding:1px 8px;border-radius:3px;font-size:11px}
.tag.used{background:#1a6b3c;color:#8fdfb0}
.tag.skip{background:#6b1a1a;color:#df8f8f}
.len{color:#888}
.bad{color:#e94560}.good{color:#4ecca3}
</style>
</head>
<body>
<div class="header">
  <h1>正文抽取评估</h1>
  <div class="stats" id="stats"></div>
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
  <span id="anchorDisplay"></span>
</div>
<script>
var DATA=[],idx=0;
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
  document.getElementById('leftPanel').innerHTML=marked.parse(d.ground,{breaks:true});
  document.getElementById('rightPanel').innerHTML=marked.parse(d.extracted,{breaks:true});
  document.getElementById('urlLink').textContent=d.url;
  document.getElementById('urlLink').href=d.url;
  document.getElementById('urlDisplay').textContent=d.url;
  var r=d.gtLen>0?(d.exLen/d.gtLen):0;
  var c=r>1.5?'bad':'good';
  document.getElementById('lenDisplay').innerHTML='原文:<span class="len">'+d.gtLen+'</span> 抽取:<span class="len">'+d.exLen+'</span> <span class="'+c+'">x'+r.toFixed(2)+'</span>';
  document.getElementById('anchorDisplay').innerHTML=d.anUsed?'<span class="tag used">anchored</span>':'<span class="tag skip">anchored跳过</span>';
  var pct=d.gtLen>0?Math.min(100,Math.round(d.exLen/d.gtLen*100)):0;
  document.getElementById('matchInfo').textContent='抽取/原文: '+pct+'%';
  var ok=0,anc=0;
  for(var j=0;j<DATA.length;j++){if(DATA[j].exLen>0)ok++;if(DATA[j].anUsed)anc++;}
  document.getElementById('stats').textContent='成功:'+ok+'/'+DATA.length+' anchored:'+anc;
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
</html>'''.replace('MARKED_JS', MARKED_JS)

class Handler(http.server.SimpleHTTPRequestHandler):
    def do_GET(self):
        p = urllib.parse.urlparse(self.path).path
        if p == '/data':
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.end_headers()
            data = json.dumps([{
                'url': e['url'], 'gtLen': e['ground_truth_len'],
                'exLen': e['extracted_len'], 'anUsed': e['anchored_used'],
                'ground': e['ground_truth'], 'extracted': e['extracted'],
            } for e in ENTRIES], ensure_ascii=False)
            self.wfile.write(data.encode())
        else:
            self.send_response(200)
            self.send_header('Content-Type', 'text/html; charset=utf-8')
            self.end_headers()
            self.wfile.write(HTML.encode())

if __name__ == '__main__':
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 45678
    with socketserver.ThreadingTCPServer(('', port), Handler) as httpd:
        print(f'评估工具已启动: http://localhost:{port}')
        print(f'共 {len(ENTRIES)} 条，← → 空格翻页')
        httpd.serve_forever()
