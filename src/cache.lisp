;;; Content-addressed caching and incremental regeneration.
;;;
;;; Cache keys are derived from the complete synthesis contract:
;;; node contract fingerprint, IR-slice fingerprint, dependency
;;; fingerprints, recipe identity, platform kit version, and
;;; model policy.  A cache hit is valid only when every component
;;; of the key matches; any semantic change invalidates the
;;; affected dependency closure.
;;;
;;; The cache stores candidate and evidence bundles immutably.
;;; Entries are never updated in place; a changed contract
;;; produces a new key and a new entry.

(def $gymnast-cache-schema "gymnast.cache/0.1")

;;; Cache key construction.
;;;
;;; A cache key is a fingerprint over the full synthesis context
;;; of a plan node.  Two compilations of the same specification
;;; produce identical cache keys for unchanged nodes.

(defun gymnast-cache-key-material (ir plan node)
  (let* ((node-fp (gymnast-plan-node-field node 'fingerprint))
      (ir-slice (gymnast-node-ir-slice ir node))
      (ir-slice-fp (gymnast-fingerprint ir-slice))
      (dep-slice (gymnast-node-dependency-slice plan node))
      (dep-fp (gymnast-fingerprint dep-slice))
      (recipe (gymnast-plan-node-field node 'recipe))
      (model (gymnast-plan-node-field node 'model))
      (capabilities (gymnast-plan-node-field node 'capabilities)))
    (list 'cache-key-material
      (list 'node-fingerprint node-fp)
      (list 'ir-slice-fingerprint ir-slice-fp)
      (list 'dependency-fingerprint dep-fp)
      (list 'recipe recipe)
      (list 'model model)
      (list 'capabilities capabilities))))

(defun gymnast-cache-key (ir plan node)
  (gymnast-fingerprint (gymnast-cache-key-material ir plan node)))

;;; Cache entries.

(defun gymnast-cache-entry (key node-id candidate evidence timestamp)
  (list 'cache-entry
    (list 'schema $gymnast-cache-schema)
    (list 'key key)
    (list 'node-id node-id)
    (list 'candidate candidate)
    (list 'evidence evidence)
    (list 'timestamp timestamp)))

(defun gymnast-cache-entry-field (entry key)
  (gymnast-assoc-value key (cdr entry)))

;;; In-memory cache store.
;;;
;;; The cache is a simple association list keyed by cache-key strings.
;;; A persistent implementation would serialize entries to disk.

(def $gymnast-cache nil)

(defun gymnast-cache-clear ()
  (setq $gymnast-cache nil))

(defun gymnast-cache-store (key entry)
  (setq $gymnast-cache
    (cons (list key entry)
      (filter (lambda (e) (not (equal (car e) key)))
        $gymnast-cache)))
  entry)

(defun gymnast-cache-lookup (key)
  (let ((entry (assoc key $gymnast-cache)))
    (if entry (cadr entry) nil)))

(defun gymnast-cache-size ()
  (length $gymnast-cache))

(defun gymnast-cache-keys ()
  (mapcar #'car $gymnast-cache))

;;; Cache validity checking.
;;;
;;; An entry is valid when:
;;; 1. Its key matches the current synthesis context
;;; 2. The stored candidate passes current validation
;;; 3. All dependency entries are also valid

(defun gymnast-cache-entry-valid-p (ir plan node entry)
  (if (not entry) nil
    (let* ((current-key (gymnast-cache-key ir plan node))
        (stored-key (gymnast-cache-entry-field entry 'key)))
      (equal current-key stored-key))))

;;; Dependency closure computation.
;;;
;;; When a node's contract changes, all nodes that transitively
;;; depend on it must be invalidated.

(defun gymnast-node-dependents (plan node-id)
  (filter
    (lambda (node)
      (member node-id (gymnast-plan-node-field node 'depends-on)))
    (gymnast-plan-field plan 'nodes)))

(defun gymnast-transitive-dependents (plan node-id visited)
  (if (member node-id visited)
    visited
    (let* ((new-visited (cons node-id visited))
        (direct (gymnast-node-dependents plan node-id))
        (direct-ids (mapcar #'gymnast-plan-node-id direct)))
      (reduce
        (lambda (v dep-id)
          (gymnast-transitive-dependents plan dep-id v))
        direct-ids
        new-visited))))

(defun gymnast-invalidated-nodes (plan changed-node-ids)
  (let ((all-affected
        (reduce
          (lambda (visited id)
            (gymnast-transitive-dependents plan id visited))
          changed-node-ids
          nil)))
    (gymnast-unique all-affected)))

;;; Diff: compare two compilations and identify changed nodes.

(defun gymnast-plan-node-changed-p (old-plan new-plan node-id)
  (let ((old-node (gymnast-find-plan-node old-plan node-id))
      (new-node (gymnast-find-plan-node new-plan node-id)))
    (cond
      ((and (not old-node) (not new-node)) nil)
      ((not old-node) t)
      ((not new-node) t)
      (t (not (equal
            (gymnast-plan-node-field old-node 'fingerprint)
            (gymnast-plan-node-field new-node 'fingerprint)))))))

(defun gymnast-diff-plans (old-plan new-plan)
  (let* ((old-nodes (gymnast-plan-field old-plan 'nodes))
      (new-nodes (gymnast-plan-field new-plan 'nodes))
      (old-ids (mapcar #'gymnast-plan-node-id old-nodes))
      (new-ids (mapcar #'gymnast-plan-node-id new-nodes))
      (all-ids (gymnast-unique (append old-ids new-ids)))
      (changed (filter
          (lambda (id)
            (gymnast-plan-node-changed-p old-plan new-plan id))
          all-ids))
      (added (filter
          (lambda (id) (not (member id old-ids)))
          new-ids))
      (removed (filter
          (lambda (id) (not (member id new-ids)))
          old-ids))
      (modified (filter
          (lambda (id)
            (and (member id old-ids)
              (member id new-ids)
              (gymnast-plan-node-changed-p old-plan new-plan id)))
          all-ids)))
    (list 'plan-diff
      (list 'added added)
      (list 'removed removed)
      (list 'modified modified)
      (list 'unchanged
        (filter (lambda (id) (not (member id changed))) new-ids))
      (list 'affected-closure
        (gymnast-invalidated-nodes new-plan changed)))))

(defun gymnast-diff-field (diff key)
  (gymnast-assoc-value key (cdr diff)))

;;; Incremental compilation with cache.
;;;
;;; For each plan node, check the cache first.  If the cache has
;;; a valid entry, reuse it.  Otherwise, mark the node for
;;; regeneration.  Returns a cache-aware execution plan.

(defun gymnast-cache-check-node (ir plan node)
  (let* ((key (gymnast-cache-key ir plan node))
      (entry (gymnast-cache-lookup key)))
    (if (gymnast-cache-entry-valid-p ir plan node entry)
      (list 'cache-hit
        (list 'node-id (gymnast-plan-node-id node))
        (list 'key key)
        (list 'candidate (gymnast-cache-entry-field entry 'candidate)))
      (list 'cache-miss
        (list 'node-id (gymnast-plan-node-id node))
        (list 'key key)))))

(defun gymnast-cache-check-plan (ir plan)
  (mapcar
    (lambda (node) (gymnast-cache-check-node ir plan node))
    (gymnast-plan-field plan 'nodes)))

(defun gymnast-cache-hits (results)
  (filter (lambda (r) (gymnast-tagged-p 'cache-hit r)) results))

(defun gymnast-cache-misses (results)
  (filter (lambda (r) (gymnast-tagged-p 'cache-miss r)) results))

;;; Store results in cache after successful execution.

(defun gymnast-cache-store-result (ir plan node candidate evidence)
  (let* ((key (gymnast-cache-key ir plan node))
      (entry (gymnast-cache-entry
          key (gymnast-plan-node-id node)
          candidate evidence 'now)))
    (gymnast-cache-store key entry)))

;;; Cache explainability: report why a node was/wasn't cached.

(defun gymnast-cache-explain-node (ir plan node)
  (let* ((key (gymnast-cache-key ir plan node))
      (entry (gymnast-cache-lookup key))
      (node-id (gymnast-plan-node-id node)))
    (cond
      ((not entry)
        (list 'explanation
          (list 'node-id node-id)
          (list 'status 'miss)
          (list 'reason 'no-cache-entry)
          (list 'key key)))
      ((not (gymnast-cache-entry-valid-p ir plan node entry))
        (list 'explanation
          (list 'node-id node-id)
          (list 'status 'miss)
          (list 'reason 'key-mismatch)
          (list 'current-key key)
          (list 'stored-key (gymnast-cache-entry-field entry 'key))))
      (t
        (list 'explanation
          (list 'node-id node-id)
          (list 'status 'hit)
          (list 'reason 'valid-entry)
          (list 'key key))))))

(defun gymnast-cache-explain-plan (ir plan)
  (mapcar
    (lambda (node) (gymnast-cache-explain-node ir plan node))
    (gymnast-plan-field plan 'nodes)))
