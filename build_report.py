# -*- coding: utf-8 -*-
"""
linux.do 当天新帖报告生成器
输入: latest_to_cutoff.json (浏览器内通过 ECH 拉取的 latest 流), cat_map.json (分类映射)
输出: 当天(本地时区 UTC+8)新发布帖子的 JSON / CSV / HTML
"""
import os
import sys
import glob
import json
import csv
import html
from datetime import datetime, timezone, timedelta
from collections import Counter

BASE = os.path.dirname(os.path.abspath(__file__))
TZ = timezone(timedelta(hours=8))

def to_local(ts: str) -> datetime:
    return datetime.fromisoformat(ts.replace("Z", "+00:00")).astimezone(TZ)

def load(path):
    with open(path, encoding="utf-8") as f:
        return json.load(f)

# 选择输入文件: 命令行参数 > latest_<今天>.json > latest_to_cutoff.json(向后兼容)
today_name = datetime.now(TZ).strftime("%Y-%m-%d")
candidates = []
if len(sys.argv) > 1:
    candidates = [sys.argv[1]]
else:
    candidates = [
        os.path.join(BASE, f"latest_{today_name}.json"),
        os.path.join(BASE, "latest_to_cutoff.json"),
    ]
    candidates += sorted(glob.glob(os.path.join(BASE, "latest_*.json")), reverse=True)
src_path = next((p for p in candidates if os.path.exists(p)), None)
if not src_path:
    raise SystemExit("未找到输入数据，请先运行 fetch_latest.js 并把下载的 json 放到本目录")

# 日期: 优先文件名中的日期; 否则取运行当天
TODAY = datetime.now(TZ).date()
m = os.path.basename(src_path)
import re
dm = re.search(r"(20\d{2}-\d{2}-\d{2})", m)
if dm:
    TODAY = datetime.strptime(dm.group(1), "%Y-%m-%d").date()

print(f"输入: {src_path}\n日期: {TODAY}")
topics = load(src_path)
cat_map = load(os.path.join(BASE, "cat_map.json"))

lo = datetime(TODAY.year, TODAY.month, TODAY.day, tzinfo=TZ)
hi = lo + timedelta(days=1)

rows = []
for t in topics:
    created = to_local(t["created_at"])
    if not (lo <= created < hi):
        continue
    cid = str(t.get("category_id"))
    tags = [x.get("name", "") for x in (t.get("tags") or [])]
    url = f"https://linux.do/t/{t.get('slug','topic')}/{t['id']}"
    rows.append({
        "id": t["id"],
        "title": t.get("title", ""),
        "url": url,
        "category": cat_map.get(cid, f"分类{cid}"),
        "category_id": cid,
        "tags": tags,
        "created_at": created.strftime("%Y-%m-%d %H:%M:%S"),
        "created_time": created.strftime("%H:%M:%S"),
        "reply_count": t.get("reply_count", 0),
        "posts_count": t.get("posts_count", 0),
        "views": t.get("views", 0),
        "like_count": t.get("like_count", 0),
        "excerpt": (t.get("excerpt") or "").replace("&hellip;", "…"),
        "pinned": bool(t.get("pinned") or t.get("pinned_globally")),
    })

# 按创建时间升序排列(从早到晚), 便于回顾当天讨论
rows.sort(key=lambda r: r["created_at"])

date_str = TODAY.strftime("%Y-%m-%d")

# 1) 干净 JSON
with open(f"{BASE}\\posts_{date_str}.json", "w", encoding="utf-8") as f:
    json.dump(rows, f, ensure_ascii=False, indent=2)

# 2) CSV (UTF-8 BOM, Excel 可直接打开)
with open(f"{BASE}\\posts_{date_str}.csv", "w", encoding="utf-8-sig", newline="") as f:
    w = csv.writer(f)
    w.writerow(["创建时间", "标题", "分类", "标签", "回复数", "浏览量", "点赞数", "链接"])
    for r in rows:
        w.writerow([r["created_at"], r["title"], r["category"], "|".join(r["tags"]),
                    r["reply_count"], r["views"], r["like_count"], r["url"]])

# 3) HTML 报告
cat_counter = Counter(r["category"] for r in rows)
total_views = sum(r["views"] for r in rows)
total_replies = sum(r["reply_count"] for r in rows)
total_likes = sum(r["like_count"] for r in rows)

cat_buttons = ['<button class="cat active" data-cat="全部">全部 (' + str(len(rows)) + ')</button>']
for cname, cnt in cat_counter.most_common():
    cat_buttons.append(f'<button class="cat" data-cat="{html.escape(cname)}">{html.escape(cname)} ({cnt})</button>')

trs = []
for r in rows:
    tag_spans = "".join(f'<span class="tag">{html.escape(x)}</span>' for x in r["tags"])
    pin = '<span class="pin">置顶</span>' if r["pinned"] else ""
    trs.append(
        '<tr data-cat="{cat}" data-title="{dtitle}">'
        '<td class="time">{time}</td>'
        '<td class="title-cell"><div class="tline">{pin}<a href="{url}" target="_blank" rel="noopener">{title}</a></div>'
        '<div class="meta"><span class="catname">{cat}</span>{tags}</div></td>'
        '<td class="num">{reply}</td><td class="num">{views}</td><td class="num">{likes}</td>'
        '</tr>'.format(
            cat=html.escape(r["category"]), dtitle=html.escape(r["title"].lower()),
            time=r["created_time"], pin=pin, url=html.escape(r["url"]),
            title=html.escape(r["title"]), tags=tag_spans,
            reply=r["reply_count"], views=r["views"], likes=r["like_count"])
    )

html_doc = """<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>LINUX DO 当天新帖 {date}</title>
<style>
:root{{--bg:#0f1115;--card:#171a21;--line:#262b36;--txt:#e6e9ef;--mut:#8b93a7;--acc:#4f9cf9;--acc2:#1f6feb;}}
*{{box-sizing:border-box;}}
body{{margin:0;background:var(--bg);color:var(--txt);font-family:"Segoe UI","Microsoft YaHei",system-ui,sans-serif;font-size:14px;}}
.wrap{{max-width:1180px;margin:0 auto;padding:28px 22px 60px;}}
h1{{font-size:24px;margin:0 0 4px;}}
.sub{{color:var(--mut);margin-bottom:22px;}}
.cards{{display:grid;grid-template-columns:repeat(4,1fr);gap:14px;margin-bottom:22px;}}
.kpi{{background:var(--card);border:1px solid var(--line);border-radius:12px;padding:16px 18px;}}
.kpi .v{{font-size:26px;font-weight:700;}}
.kpi .l{{color:var(--mut);font-size:13px;margin-top:4px;}}
.toolbar{{display:flex;flex-wrap:wrap;gap:8px;align-items:center;margin-bottom:14px;}}
#search{{flex:1;min-width:220px;background:var(--card);border:1px solid var(--line);color:var(--txt);
border-radius:9px;padding:10px 14px;font-size:14px;outline:none;}}
#search:focus{{border-color:var(--acc);}}
.cat{{background:var(--card);border:1px solid var(--line);color:var(--txt);border-radius:999px;
padding:7px 14px;cursor:pointer;font-size:13px;}}
.cat:hover{{border-color:var(--acc);}}
.cat.active{{background:var(--acc2);border-color:var(--acc2);color:#fff;}}
table{{width:100%;border-collapse:collapse;background:var(--card);border:1px solid var(--line);border-radius:12px;overflow:hidden;}}
th,td{{padding:11px 14px;text-align:left;border-bottom:1px solid var(--line);vertical-align:top;}}
th{{background:#12151c;color:var(--mut);font-weight:600;font-size:13px;position:sticky;top:0;}}
tr:last-child td{{border-bottom:none;}}
.time{{white-space:nowrap;color:var(--mut);width:78px;}}
.num{{text-align:right;white-space:nowrap;width:70px;}}
.title-cell{{min-width:380px;}}
.tline a{{color:var(--txt);text-decoration:none;font-weight:600;line-height:1.5;}}
.tline a:hover{{color:var(--acc);}}
.meta{{margin-top:5px;display:flex;flex-wrap:wrap;gap:6px;align-items:center;}}
.catname{{color:var(--acc);font-size:12px;background:rgba(79,156,249,.12);padding:2px 8px;border-radius:6px;}}
.tag{{color:var(--mut);font-size:12px;background:#0f1218;border:1px solid var(--line);padding:2px 8px;border-radius:6px;}}
.pin{{font-size:11px;color:#ffb020;border:1px solid #ffb020;border-radius:5px;padding:1px 6px;margin-right:6px;}}
#count{{color:var(--mut);margin:10px 2px 0;font-size:13px;}}
.foot{{color:var(--mut);font-size:12px;margin-top:26px;line-height:1.7;}}
</style>
</head>
<body>
<div class="wrap">
  <h1>LINUX DO · 当天新发布帖子</h1>
  <div class="sub">日期 {date}（本地时区 UTC+8） · 数据来源 linux.do Discourse /latest.json · 经本地 ECH 加密通道获取</div>
  <div class="cards">
    <div class="kpi"><div class="v">{n}</div><div class="l">新帖总数</div></div>
    <div class="kpi"><div class="v">{views}</div><div class="l">累计浏览量</div></div>
    <div class="kpi"><div class="v">{replies}</div><div class="l">累计回复数</div></div>
    <div class="kpi"><div class="v">{likes}</div><div class="l">累计点赞数</div></div>
  </div>
  <div class="toolbar">{buttons}</div>
  <input id="search" type="text" placeholder="搜索标题关键词…（回车或输入即筛选）">
  <div id="count"></div>
  <table>
    <thead><tr><th>时间</th><th>标题 / 分类 / 标签</th><th>回复</th><th>浏览</th><th>点赞</th></tr></thead>
    <tbody id="tb">{rows}</tbody>
  </table>
  <div class="foot">
    说明：本报告仅统计“当天新创建”的话题（不含旧帖被顶起）。点击标题可在已通过 Cloudflare 验证的浏览器中打开原帖。<br>
    访问链路依赖本地 linuxdo-accelerator（ECH 加密 SNI），无需 VPN / Clash。
  </div>
</div>
<script>
const rows=[...document.querySelectorAll('#tb tr')];
let curCat='全部';
function apply(){{
  const q=document.getElementById('search').value.trim().toLowerCase();
  let shown=0;
  rows.forEach(tr=>{{
    const okCat=(curCat==='全部'||tr.dataset.cat===curCat);
    const okQ=(!q||tr.dataset.title.includes(q));
    const vis=okCat&&okQ;
    tr.style.display=vis?'':'none';
    if(vis)shown++;
  }});
  document.getElementById('count').textContent='显示 '+shown+' / '+rows.length+' 条';
}}
document.querySelectorAll('.cat').forEach(b=>b.addEventListener('click',()=>{{
  document.querySelectorAll('.cat').forEach(x=>x.classList.remove('active'));
  b.classList.add('active');curCat=b.dataset.cat;apply();
}}));
document.getElementById('search').addEventListener('input',apply);
apply();
</script>
</body>
</html>""".format(
    date=date_str, n=len(rows), views=total_views, replies=total_replies,
    likes=total_likes, buttons="".join(cat_buttons), rows="".join(trs))

with open(f"{BASE}\\posts_{date_str}.html", "w", encoding="utf-8") as f:
    f.write(html_doc)

print(f"当天新帖: {len(rows)} 条")
print(f"分类分布: {dict(cat_counter)}")
print(f"累计浏览 {total_views} / 回复 {total_replies} / 点赞 {total_likes}")
print("已生成:")
for ext in ("json", "csv", "html"):
    print(f"  posts_{date_str}.{ext}")
