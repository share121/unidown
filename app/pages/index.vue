<template>
  <div class="grid h-screen place-items-center">
    <div class="w-full max-w-3xl p-2">
      <h1 class="my-8 text-center text-5xl font-bold">Unidown</h1>
      <Form class="mb-4 flex gap-2" @submit="submitHandle">
        <InputText
          class="flex-1"
          fluid
          name="input"
          type="text"
          :disabled="loading"
          placeholder="请输入待解析的 URL"
        />
        <Button type="submit" :loading label="解析" />
      </Form>
      <Fieldset v-if="title" :legend="title">
        <Panel header="合并版" class="mb-2" :collapsed="!mergeUrl">
          <template #icons>
            <Button
              v-if="mergeUrl"
              as="a"
              :href="mergeUrl"
              :download="`${title}.mp4`"
              icon="pi pi-download"
              severity="secondary"
              rounded
              text
              size="small"
            />
          </template>
          <video v-if="mergeUrl" class="w-full" :src="mergeUrl" controls />
        </Panel>
        <Panel header="视频" class="mb-2" :collapsed="!videoUrl">
          <template #icons>
            <Button
              v-if="videoUrl"
              as="a"
              :href="videoUrl"
              :download="`${title}.mp4`"
              icon="pi pi-download"
              severity="secondary"
              rounded
              text
              size="small"
            />
          </template>
          <video v-if="videoUrl" class="w-full" :src="videoUrl" controls />
        </Panel>
        <Panel header="音频" :collapsed="!audioUrl">
          <template #icons>
            <Button
              v-if="audioUrl"
              as="a"
              :href="audioUrl"
              :download="`${title}.mp3`"
              icon="pi pi-download"
              severity="secondary"
              rounded
              text
              size="small"
            />
          </template>
          <audio v-if="audioUrl" class="w-full" :src="audioUrl" controls />
        </Panel>
      </Fieldset>
    </div>
  </div>
</template>

<script setup lang="ts">
import type { FormSubmitEvent } from "@primevue/forms";
import { FFmpeg } from "@ffmpeg/ffmpeg";

const loading = ref(false);
const toast = useToast();
const title = ref("");
const videoUrl = ref("");
const audioUrl = ref("");
const videoBlob = ref<Blob>();
const audioBlob = ref<Blob>();
const mergeUrl = ref("");
let ffmpeg: FFmpeg | null = null;

onMounted(async () => {
  ffmpeg = new FFmpeg();
  await ffmpeg.load();
  toast.add({ severity: "info", summary: "FFmpeg 加载完成", life: 3000 });
});
watch(videoUrl, (_, oldVal) => {
  if (oldVal) URL.revokeObjectURL(oldVal);
});
watch(audioUrl, (_, oldVal) => {
  if (oldVal) URL.revokeObjectURL(oldVal);
});
watch(mergeUrl, (_, oldVal) => {
  if (oldVal) URL.revokeObjectURL(oldVal);
});
onBeforeUnmount(() => {
  if (videoUrl.value) URL.revokeObjectURL(videoUrl.value);
  if (audioUrl.value) URL.revokeObjectURL(audioUrl.value);
  if (mergeUrl.value) URL.revokeObjectURL(mergeUrl.value);
});

async function submitHandle(e: FormSubmitEvent) {
  if (!e.valid) return;
  loading.value = true;
  try {
    const input = e.values.input as string;
    const res = await $fetch("/api/extract", {
      method: "POST",
      body: { input },
    });
    console.log(res);
    if (isVideoInfo(res)) {
      title.value = res.title;
      toast.add({
        severity: "success",
        summary: "解析成功，开始下载",
        life: 3000,
      });
      const [videoBlobLocal, audioBlobLocal] = await Promise.all([
        $fetch<Blob>("https://unidown-fetch.s121.top", {
          method: "POST",
          body: {
            url: res.videoUrl,
            headers: res.headers,
          },
        }).then((res) => {
          videoBlob.value = res;
          videoUrl.value = URL.createObjectURL(res);
          toast.add({
            severity: "success",
            summary: "视频下载成功",
            life: 3000,
          });
          return res;
        }),
        res.audioUrl
          ? $fetch<Blob | undefined>("https://unidown-fetch.s121.top", {
              method: "POST",
              body: {
                url: res.audioUrl,
                headers: res.headers,
              },
            }).then((res) => {
              audioBlob.value = res;
              audioUrl.value = res ? URL.createObjectURL(res) : "";
              toast.add({
                severity: "success",
                summary: "音频下载成功",
                life: 3000,
              });
              return res;
            })
          : undefined,
      ]);
      if (audioBlobLocal) {
        const mergeMedia = await mergeVideo(videoBlobLocal, audioBlobLocal);
        mergeUrl.value = URL.createObjectURL(mergeMedia);
      } else {
        mergeUrl.value = "";
      }
    } else {
      toast.add({
        severity: "error",
        summary: "解析失败",
        detail: res,
        contentStyleClass: "whitespace-pre-wrap",
        closable: true,
      });
    }
  } finally {
    loading.value = false;
  }
}
// eslint-disable-next-line @typescript-eslint/no-explicit-any
function isVideoInfo(obj: any): obj is VideoInfo {
  return "videoUrl" in obj;
}

async function mergeVideo(videoBlob: Blob, audioBlob: Blob): Promise<Blob> {
  if (!ffmpeg) ffmpeg = new FFmpeg();
  await ffmpeg.load();
  const [videoData, audioData] = await Promise.all([
    videoBlob.arrayBuffer(),
    audioBlob.arrayBuffer(),
  ]);
  await ffmpeg.writeFile("input.mp4", new Uint8Array(videoData));
  await ffmpeg.writeFile("input.mp3", new Uint8Array(audioData));
  await ffmpeg.exec([
    "-i",
    "input.mp4",
    "-i",
    "input.mp3",
    "-c",
    "copy",
    "-shortest",
    "output.mp4",
  ]);
  const outputData = await ffmpeg.readFile("output.mp4");
  const outputBlob = new Blob([outputData.slice()], { type: "video/mp4" });
  await ffmpeg.deleteFile("input.mp4");
  await ffmpeg.deleteFile("input.mp3");
  await ffmpeg.deleteFile("output.mp4");
  return outputBlob;
}
</script>
