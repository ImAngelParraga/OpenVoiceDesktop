const labels = {
  general: "General",
  asr: "ASR Endpoint",
  advanced: "Advanced",
  history: "History",
  debug: "Debug",
  about: "About",
};

const content = document.querySelector("#content");
for (const button of document.querySelectorAll("nav button")) {
  button.addEventListener("click", () => {
    document.querySelector("nav button.active")?.classList.remove("active");
    button.classList.add("active");
    content.replaceChildren();
    const heading = document.createElement("h2");
    heading.textContent = labels[button.dataset.section];
    const status = document.createElement("p");
    status.textContent = "Settings API loading…";
    content.append(heading, status);
    content.focus();
  });
}
