/* ============================================================
 * LINUX DO 当天新帖抓取脚本（在已通过 Cloudflare 验证的浏览器中使用）
 * 用法：
 *   1. 先打开 https://linux.do 并等待主页正常加载（CF 挑战自动通过）
 *   2. 按 F12 打开开发者工具，切到“控制台/Console”
 *   3. 若提示禁止粘贴，先输入 允许粘贴 回车；然后粘贴本文件全部内容，回车
 * 结果：自动翻页直到“本地今天零点”，下载 latest_<日期>.json
 * 之后可运行 build_report.py 生成 JSON/CSV/HTML 报告
 * ============================================================ */
(async () => {
  // 本地时区的“今天零点”
  const now = new Date();
  const cutoff = new Date(now.getFullYear(), now.getMonth(), now.getDate(), 0, 0, 0).getTime();
  const pad = (n) => String(n).padStart(2, "0");
  const dateStr = `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;

  const all = {};
  let reached = false;
  for (let p = 0; p < 40 && !reached; p++) {
    const r = await fetch(`/latest.json?no_definitions=true&per_page=100&page=${p}`, {
      headers: { Accept: "application/json" },
    });
    if (!r.ok) { console.log("page", p, "HTTP", r.status); break; }
    const j = await r.json();
    const ts = j.topic_list.topics;
    console.log("page", p, "got", ts.length);
    if (!ts || ts.length === 0) break;
    for (const t of ts) {
      all[t.id] = t;
      if (Date.parse(t.bumped_at) < cutoff) reached = true; // 越过零点，之后不会再有今天的新帖
    }
  }

  const arr = Object.values(all);
  console.log("TOTAL", arr.length, "REACHED_CUTOFF", reached);
  const blob = new Blob([JSON.stringify(arr)], { type: "application/json" });
  const a = document.createElement("a");
  a.href = URL.createObjectURL(blob);
  a.download = `latest_${dateStr}.json`;
  a.click();
  console.log("DONE 下载", arr.length, "条 ->", a.download);
})();
