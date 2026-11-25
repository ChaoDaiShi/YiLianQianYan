import tailwindcss from "tailwindcss";
import autoprefixer from "autoprefixer";

// Apply the theme's weight scale to both authored CSS and Tailwind utilities.
const themeFontWeights = {
  postcssPlugin: "theme-font-weights",
  OnceExit(root) {
    const adjusted = weight => `max(100, calc(${weight} - var(--dark-font-weight-step, 0)))`;
    root.walkDecls(/^font(-weight)?$/, decl => {
      if (/[\\/]node_modules[\\/]/.test(decl.source?.input.file ?? "")) return;
      if (decl.prop === "font-weight") {
        const weight = { normal: "400", bold: "700" }[decl.value] ?? decl.value;
        if (/^[1-9]\d{2}$/.test(weight)) decl.value = adjusted(weight);
      } else {
        decl.value = decl.value.replace(/^[1-9]\d{2}(?=\s)/, adjusted);
      }
    });
  },
};

export default { plugins: [tailwindcss(), autoprefixer(), themeFontWeights] };
