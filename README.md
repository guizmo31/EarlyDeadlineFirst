# EDF Scheduler Simulator

Simulateur de l'algorithme d'ordonnancement **Early Deadline First (EDF)** avec support multi-core, écrit en Rust avec une interface web interactive.

## L'algorithme EDF

### Principe

L'algorithme **Early Deadline First** est un algorithme d'ordonnancement préemptif à priorité dynamique. À chaque instant, le processeur exécute la tâche dont l'échéance (deadline) est la plus proche. C'est un algorithme optimal pour les systèmes mono-processeur : si un ensemble de tâches périodiques est ordonnançable, EDF trouvera un ordonnancement valide.

### Condition de faisabilité (mono-core)

Un ensemble de tâches périodiques est ordonnançable par EDF sur un seul processeur si et seulement si l'utilisation totale du CPU est inférieure ou égale à 100% :

```
U = Σ (Ci / Ti) ≤ 1
```

Où `Ci` est le temps CPU requis et `Ti` la période de la tâche `i`.

### Extension multi-core

Sur un système à `M` cores, le simulateur implémente un ordonnancement **global EDF** :

1. **Tri EDF** : À chaque tick, toutes les tâches prêtes sont triées par deadline croissante. En cas d'égalité, la priorité statique (0 = plus prioritaire) puis l'index du process servent de départage.

2. **Affectation en deux passes** :
   - **Passe 1 — Tâches pinned** : Les tâches avec core pinning sont assignées en priorité à leur core désigné (par ordre EDF).
   - **Passe 2 — Tâches libres** : Les tâches non-pinned remplissent les cores restants dans l'ordre (Core 0, Core 1, ...), réalisant un load balancing naturel.

3. **Préemption** : À chaque tick d'ordonnancement, les tâches peuvent être préemptées si une tâche avec une deadline plus proche devient disponible.

4. **Détection des deadline misses** : Si une tâche n'a pas terminé son exécution avant sa deadline, un événement "deadline miss" est enregistré.

### Core Pinning

Le core pinning (affinité processeur) permet de forcer l'exécution d'une tâche sur un core spécifique :
- **Tâche pinned** : s'exécute exclusivement sur le core désigné.
- **Tâche non-pinned** : est assignée au premier core disponible (load balancing).

Cela permet de simuler des scénarios réels comme l'isolation de tâches critiques sur un core dédié.

## Structure du projet

```
EarlyDeadlineFirst/
├── Cargo.toml              # Workspace Rust
├── .cargo/config.toml      # Configuration toolchain (GNU)
├── edf-core/               # Bibliothèque Rust — moteur EDF
│   └── src/lib.rs          # Algorithme de simulation
├── edf-server/             # Binaire Rust — serveur HTTP (actix-web)
│   └── src/main.rs         # API REST + serveur de fichiers statiques
└── edf-web/                # Frontend — HTML/CSS/JS (vanilla)
    ├── index.html           # Page principale
    ├── style.css            # Styles (thème sombre)
    └── app.js               # Logique frontend + rendu Gantt (Canvas)
```

## Prérequis

- **Rust** (stable) — installable via [rustup](https://rustup.rs/)
- **MinGW** (sur Windows avec toolchain GNU) — `scoop install mingw` ou équivalent

## Compilation

```bash
# Cloner le dépôt
git clone <url-du-repo>
cd EarlyDeadlineFirst

# Compiler en mode release
cargo build --release -p edf-server

# Lancer les tests unitaires
cargo test -p edf-core
```

### Sur Windows (Git Bash / MSYS2)

Si vous utilisez la toolchain GNU, assurez-vous que MinGW est dans le PATH :

```bash
export PATH="$HOME/scoop/apps/mingw/current/bin:$HOME/.cargo/bin:$PATH"
cargo build --release -p edf-server
```

## Lancer le simulateur

```bash
cargo run --release -p edf-server
```

Le serveur démarre sur **http://localhost:8080**. Ouvrez cette URL dans votre navigateur.

## Utilisation du simulateur

### Configuration

1. **Tick Period (ms)** : Granularité de l'ordonnancement. 1 ms = précision maximale.
2. **Simulation Duration (ms)** : Durée totale de la simulation.
3. **CPU Cores** : Nombre de cœurs processeur (1 à 16).

### Définition des processus

Pour chaque processus, vous pouvez configurer :
- **Nom** : Identifiant du processus.
- **Period (ms)** : Période de la tâche (intervalle entre deux activations).
- **CPU time (ms)** : Temps CPU requis par période.
- **Priority** : Priorité statique (0 = plus haute). Sert de départage quand deux tâches ont la même deadline.
- **Color** : Couleur dans le diagramme de Gantt.
- **Core Pinning** : Si coché, le processus est forcé sur le core spécifié.

### Diagramme de Gantt

Après simulation, le diagramme de Gantt affiche deux sections :

- **CORES** : Vue par cœur — chaque ligne montre l'activité d'un core (quel processus s'exécute à chaque instant).
- **PROCESSES** : Vue par processus — chaque ligne montre quand un processus s'exécute (et sur quel core, indiqué par `C0`, `C1`, ...).

Les marqueurs visuels incluent :
- Blocs colorés par processus, blocs blancs pour IDLE.
- Lignes pointillées verticales pour les périodes de chaque processus.
- Triangles rouges pour les deadline misses.
- Tooltip au survol avec les détails (start, duration, end, core).

### Statistiques

- **Theoretical Utilization** : Somme des ratios CPU/période (indicateur de faisabilité).
- **Per-core utilization** : Pourcentage d'occupation de chaque core.
- **Global CPU Used** : Utilisation globale sur l'ensemble des cores.
- **Deadline Misses** : Nombre de deadlines manquées (avec détails au survol).

## API REST

Le serveur expose deux endpoints :

### `GET /api/health`
Vérification de l'état du serveur.

### `POST /api/simulate`
Lance une simulation EDF. Corps de la requête (JSON) :

```json
{
  "tick_period_ms": 1,
  "simulation_duration_ms": 120,
  "num_cores": 2,
  "processes": [
    {
      "name": "Process-A",
      "period_ms": 10,
      "cpu_time_ms": 2,
      "priority": 0,
      "pinned_core": null
    },
    {
      "name": "Process-B",
      "period_ms": 30,
      "cpu_time_ms": 10,
      "priority": 1,
      "pinned_core": 1
    }
  ]
}
```

Réponse :

```json
{
  "schedule": [
    { "time_ms": 0, "duration_ms": 2, "process_name": "Process-A", "core": 0 },
    { "time_ms": 2, "duration_ms": 8, "process_name": "IDLE", "core": 0 }
  ],
  "total_duration_ms": 120,
  "cpu_utilization": 0.533,
  "num_cores": 2,
  "deadline_misses": []
}
```

## Exemple classique

Configuration par défaut du simulateur :

| Processus | Période | CPU Time | Utilisation |
|-----------|---------|----------|-------------|
| Process-A | 10 ms   | 2 ms     | 20%         |
| Process-B | 30 ms   | 10 ms    | 33.3%       |
| Process-C | 60 ms   | 20 ms    | 33.3%       |
| **Total** |         |          | **86.7%**   |

Avec U = 86.7% < 100%, cet ensemble est ordonnançable par EDF sur 1 core sans aucune deadline miss.

## Licence

MIT
