<script lang="ts" module>
  // Outline icons on a 24px grid, drawn with the current text color.
  const paths = {
    phone: "M8 2.75h8A1.75 1.75 0 0 1 17.75 4.5v15A1.75 1.75 0 0 1 16 21.25H8A1.75 1.75 0 0 1 6.25 19.5v-15A1.75 1.75 0 0 1 8 2.75ZM10.5 18h3",
    usb: "M12 3v12M12 3l-2 2.5M12 3l2 2.5M12 15a2 2 0 1 0 0 4 2 2 0 0 0 0-4ZM8 8v2.5l4 2.5M16 7h-1v3.5L12 12.5M7 7h2v1H7z",
    wifi: "M4 9.5a11.5 11.5 0 0 1 16 0M7 12.75a7 7 0 0 1 10 0M9.75 15.75a2.75 2.75 0 0 1 4.5 0M12 19h.01",
    gear: "M12 9.25a2.75 2.75 0 1 0 0 5.5 2.75 2.75 0 0 0 0-5.5ZM10.4 3.3h3.2l.5 2.2 1.6.9 2.1-.7 1.6 2.8-1.6 1.5v1.9l1.6 1.5-1.6 2.8-2.1-.7-1.6.9-.5 2.2h-3.2l-.5-2.2-1.6-.9-2.1.7-1.6-2.8L6.2 13v-1.9L4.6 9.5l1.6-2.8 2.1.7 1.6-.9Z",
    back: "M15 5l-7 7 7 7",
    switch: "M4.75 8.5A1.75 1.75 0 0 1 6.5 6.75h2L10 5h4l1.5 1.75h2a1.75 1.75 0 0 1 1.75 1.75v8.75a1.75 1.75 0 0 1-1.75 1.75h-11a1.75 1.75 0 0 1-1.75-1.75ZM9 12.5a3 3 0 0 1 5.2-2M15 11.5a3 3 0 0 1-5.2 2M14.5 8.8v1.9h-1.9M9.5 15.2v-1.9h1.9",
    mirror: "M12 3.5v17M9.5 6.5 4 17.5h5.5ZM14.5 6.5 20 17.5h-5.5Z",
    torch: "M8 3.75h8v3L13.75 10v10.25h-3.5V10L8 6.75ZM12 13v2",
    zoomIn: "M10.5 4.75a5.75 5.75 0 1 0 0 11.5 5.75 5.75 0 0 0 0-11.5ZM14.75 14.75 20 20M10.5 8v5M8 10.5h5",
    zoomOut: "M10.5 4.75a5.75 5.75 0 1 0 0 11.5 5.75 5.75 0 0 0 0-11.5ZM14.75 14.75 20 20M8 10.5h5",
    lens: "M12 4.75a7.25 7.25 0 1 0 0 14.5 7.25 7.25 0 0 0 0-14.5ZM12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6Z",
    close: "M6 6l12 12M18 6 6 18",
    video: "M3.75 7.5A1.75 1.75 0 0 1 5.5 5.75h8A1.75 1.75 0 0 1 15.25 7.5v9a1.75 1.75 0 0 1-1.75 1.75h-8a1.75 1.75 0 0 1-1.75-1.75ZM15.25 10.25l5-3v9.5l-5-3",
    info: "M12 3.75a8.25 8.25 0 1 0 0 16.5 8.25 8.25 0 0 0 0-16.5ZM12 11v5M12 8h.01",
    check: "M5 12.5l4.5 4.5L19 7.5",
    chevron: "M8 10l4 4 4-4",
    external: "M13.5 4.75h5.75v5.75M19.25 4.75 11 13M17 14v4.25A1 1 0 0 1 16 19.25H5.75a1 1 0 0 1-1-1V8a1 1 0 0 1 1-1H10",
  } as const;
  export type IconName = keyof typeof paths;
</script>

<script lang="ts">
  let { name, size = 20 }: { name: IconName; size?: number } = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="1.6"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"><path d={paths[name]} /></svg>
