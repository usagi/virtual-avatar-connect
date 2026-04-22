const ch = new URLSearchParams(location.search).get("channel") || "ai";
let lastId = 0;
async function poll() {
 try {
  const r = await fetch(location.origin + "/output", {
   method: "POST",
   headers: { "Content-Type": "application/json" },
   body: JSON.stringify({ channels: [{ name: ch, retrieved_id: lastId, count: 2 }] }),
  });
  const j = await r.json();
  const arr = (j.channel_data && j.channel_data[ch]) || [];
  if (arr.length) {
   const last = arr[arr.length - 1];
   if (last.id != null) lastId = last.id;
   if (last.content) document.getElementById("line").textContent = last.content;
  }
 } catch (e) {}
 setTimeout(poll, 400);
}
poll();
