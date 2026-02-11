import * as Douyin from "./index.js";

async function parse(url) {
  let { body, auth } = Douyin.genReq(url);
  let resp = await fetch("https://www.hellotik.app/api/parse", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "X-Auth-Token": auth,
    },
    body,
  }).then((res) => res.text());
  return Douyin.genOuput(resp);
}

parse("https://www.douyin.com/jingxuan?modal_id=7604181359375977755")
  .then(console.log)
  .catch(console.error);
