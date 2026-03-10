// Copyright (c) 2026 Ivan LE HIN — CC BY-NC-SA 4.0
// Shared navigation bar component for all EDF tools.

(function () {
    "use strict";

    const PAGES = {
        "/module/": "Modules",
        "/builder/": "Topology Builder",
        "/scheduler/": "Configure Scheduler",
        "/viewer/": "Play Scheduler",
    };

    function currentPath() {
        return window.location.pathname.replace(/\/+$/, "/");
    }

    function hasTopology() {
        return !!localStorage.getItem("edf-topology-name");
    }

    function hasSchedulerConfig() {
        return localStorage.getItem("edf-has-scheduler-config") === "true";
    }

    function topologyName() {
        return localStorage.getItem("edf-topology-name") || "";
    }

    function render() {
        const nav = document.getElementById("tool-nav");
        if (!nav) return;

        const path = currentPath();
        const topoLoaded = hasTopology();
        const configLoaded = hasSchedulerConfig();
        const topoName = topologyName();

        // Determine active state
        const isModules = path.startsWith("/module");
        const isBuilder = path.startsWith("/builder");
        const isScheduler = path.startsWith("/scheduler");
        const isViewer = path.startsWith("/viewer");
        const isTopologyPage = isBuilder || isScheduler || isViewer;

        nav.innerHTML = "";

        // --- Modules link ---
        const modulesLink = document.createElement("a");
        modulesLink.href = "/module/";
        modulesLink.textContent = "Modules";
        if (isModules) modulesLink.classList.add("active");
        nav.appendChild(modulesLink);

        // --- Topology dropdown ---
        const dropdown = document.createElement("div");
        dropdown.className = "nav-dropdown" + (isTopologyPage ? " active" : "");

        const trigger = document.createElement("a");
        trigger.href = "#";
        trigger.className = "nav-dropdown-trigger" + (isTopologyPage ? " active" : "");
        trigger.innerHTML = "Topology &#9662;";
        trigger.addEventListener("click", (e) => {
            e.preventDefault();
            dropdown.classList.toggle("open");
        });
        dropdown.appendChild(trigger);

        const menu = document.createElement("div");
        menu.className = "nav-dropdown-menu";

        // Builder — always accessible
        const builderLink = createSubLink("/builder/", "Builder", isBuilder);
        menu.appendChild(builderLink);

        // Scheduler — needs topology
        const schedulerLink = createSubLink("/scheduler/", "Configure Scheduler", isScheduler, !topoLoaded);
        if (!topoLoaded) schedulerLink.title = "Load or create a topology first";
        menu.appendChild(schedulerLink);

        // Simulator — needs topology + scheduler config
        const viewerLink = createSubLink("/viewer/", "Play Scheduler", isViewer, !configLoaded);
        if (!configLoaded) viewerLink.title = "Run a simulation first";
        menu.appendChild(viewerLink);

        dropdown.appendChild(menu);
        nav.appendChild(dropdown);

        // --- Topology badge (if a topology is loaded) ---
        if (topoName) {
            const topoVer = localStorage.getItem("edf-topology-version") || "";
            const badge = document.createElement("span");
            badge.className = "nav-topology-badge";
            badge.textContent = topoVer ? `${topoName} v${topoVer}` : topoName;
            badge.title = "Active topology";
            nav.appendChild(badge);
        }

        // Close dropdown when clicking outside
        document.addEventListener("click", (e) => {
            if (!dropdown.contains(e.target)) {
                dropdown.classList.remove("open");
            }
        });
    }

    function createSubLink(href, label, isActive, isDisabled) {
        const a = document.createElement("a");
        a.href = href;
        a.textContent = label;
        if (isActive) a.classList.add("active");
        if (isDisabled) {
            a.classList.add("disabled");
            a.addEventListener("click", (e) => e.preventDefault());
        }
        return a;
    }

    // Render on DOM ready
    if (document.readyState === "loading") {
        document.addEventListener("DOMContentLoaded", render);
    } else {
        render();
    }

    // Expose for re-render (when localStorage changes)
    window.edfNavRefresh = render;
})();
