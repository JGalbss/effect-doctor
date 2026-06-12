import { defineConfig } from "astro/config"

export default defineConfig({
  site: "https://effect-doctor.dev",
  markdown: {
    shikiConfig: {
      theme: "github-dark-default",
    },
  },
})
