# Plan : EDF Scheduler en Rust + Simulateur Web

## Prérequis

### Étape 0 — Installer Rust
Rust n'est pas installé sur cette machine. Il faudra l'installer via `rustup` avant de commencer.
```
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```
(Ou via l'installeur Windows : https://rustup.rs)

---

## Architecture globale

```
EarlyDeadlineFirst/
├── edf-core/          # Bibliothèque Rust — moteur EDF
├── edf-server/        # Serveur HTTP Rust (API REST + sert le frontend)
└── edf-web/           # Frontend web (HTML/CSS/JS) — IHM graphique
```

**Principe** : Le moteur EDF tourne côté Rust, exposé via une API REST.
Le frontend web appelle cette API et affiche le chronogramme (diagramme de Gantt).

---

## Phase 1 — Moteur EDF (`edf-core`)

### Étape 1.1 : Initialiser le projet Rust (workspace)
- `cargo init --lib edf-core` dans un workspace Cargo
- Créer `Cargo.toml` racine avec workspace members

### Étape 1.2 : Modèle de données
Structures Rust :
```rust
struct ProcessConfig {
    name: String,        // Ex: "Process-A"
    period_ms: u64,      // Période de scheduling (ex: 10 ms)
    cpu_time_ms: u64,    // Temps CPU requis par période (ex: 2 ms)
}

struct SchedulerConfig {
    tick_period_ms: u64,          // Tick du scheduler (ex: 10 ms)
    simulation_duration_ms: u64,  // Durée totale de simulation
    processes: Vec<ProcessConfig>,
}

struct ScheduleEntry {
    time_ms: u64,        // Instant de début
    duration_ms: u64,    // Durée d'exécution dans ce slot
    process_name: String,// Quel process tourne (ou "IDLE")
}

struct SimulationResult {
    schedule: Vec<ScheduleEntry>,
    total_duration_ms: u64,
    cpu_utilization: f64,
    deadline_misses: Vec<DeadlineMiss>,
}
```

### Étape 1.3 : Algorithme EDF
Implémenter le cœur de l'algorithme :
1. À chaque tick du scheduler, évaluer les deadlines de tous les process actifs
2. Le process avec la deadline la plus proche (earliest deadline) est élu
3. Si un process de plus haute priorité (deadline plus proche) arrive → préemption
4. Détecter les deadline misses (quand un process n'a pas fini avant sa prochaine période)
5. Gérer le temps IDLE (aucun process prêt)

### Étape 1.4 : Tests unitaires
- Test avec l'exemple donné (A=10/2, B=30/10, C=60/20)
- Test de surcharge CPU (utilisation > 100%)
- Test avec un seul process
- Test de préemption

---

## Phase 2 — Serveur API REST (`edf-server`)

### Étape 2.1 : Initialiser le serveur
- `cargo init edf-server`
- Dépendances : `actix-web` (serveur HTTP), `serde`/`serde_json` (sérialisation)
- Le serveur sert aussi les fichiers statiques du frontend

### Étape 2.2 : Endpoints API
```
POST /api/simulate
  Body: { tick_period_ms, simulation_duration_ms, processes: [...] }
  Response: { schedule: [...], cpu_utilization, deadline_misses: [...] }

GET /api/health
  Response: { status: "ok" }
```

### Étape 2.3 : Servir le frontend
- Servir les fichiers de `edf-web/` sur `GET /` (fichiers statiques)

---

## Phase 3 — Frontend Web (`edf-web`)

### Étape 3.1 : Structure de l'IHM
HTML/CSS/JS vanilla (pas de framework lourd — simplicité).
L'interface comprend :

1. **Panel de configuration** (gauche/haut) :
   - Champ "Tick Period (ms)" — input numérique
   - Champ "Simulation Duration (ms)" — input numérique
   - Liste des process avec pour chacun :
     - Nom (texte)
     - Période (ms)
     - Durée CPU (ms)
     - Bouton supprimer (×)
   - Bouton "Ajouter un process"
   - Bouton "Simuler"
   - Affichage du taux d'utilisation CPU total

2. **Chronogramme** (centre/bas) :
   - Diagramme de Gantt horizontal
   - Axe X = temps (ms)
   - Une ligne par process + une ligne IDLE
   - Couleurs distinctes par process
   - Marqueurs de deadline miss (rouge)
   - Tooltip au survol (détails du slot)

### Étape 3.2 : Rendu du chronogramme
- Utiliser `<canvas>` HTML5 pour le dessin du Gantt
- Zoom / scroll horizontal pour les longues simulations
- Légende avec couleurs

### Étape 3.3 : Interaction avec l'API
- Appel `fetch()` vers `POST /api/simulate` au clic sur "Simuler"
- Mise à jour dynamique du chronogramme avec la réponse
- Validation côté client (périodes > 0, durée CPU ≤ période, etc.)

---

## Phase 4 — Intégration et finitions

### Étape 4.1 : Script de lancement
- Un seul `cargo run` dans `edf-server` démarre tout
- Ouvre automatiquement le navigateur sur `http://localhost:8080`

### Étape 4.2 : Gestion d'erreurs
- Affichage clair si utilisation CPU > 100% (non-schedulable)
- Messages d'erreur lisibles côté frontend

### Étape 4.3 : README
- Instructions d'installation et d'utilisation

---

## Résumé des technologies
| Composant | Technologie |
|-----------|------------|
| Moteur EDF | Rust (lib pure) |
| Serveur API | Rust + actix-web |
| Frontend | HTML5 + CSS + JavaScript vanilla + Canvas |
| Communication | REST JSON |

## Ordre d'implémentation
1. Installer Rust (prérequis)
2. Phase 1 : Moteur EDF (cœur algorithmique)
3. Phase 2 : Serveur API
4. Phase 3 : Frontend web
5. Phase 4 : Intégration
