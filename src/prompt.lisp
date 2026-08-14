;;; Stable prompt compilation from node contracts.
;;;
;;; A prompt is a projection of an authoritative node contract.  Editing the
;;; prose cannot change the contract; changing the contract changes its
;;; fingerprint and therefore the prompt fingerprint.
;;;
;;; Prompt structure follows VLSI synthesis discipline: the model receives
;;; characterized capability contracts (cell library), structured behavioral
;;; reference (golden model), type constraints, and state model properties
;;; rather than a raw IR dump.  The generation space is constrained by the
;;; contract, not by a polite request.

(defun gymnast-node-ir-slice (ir node)
  (filter
    (lambda (x) x)
    (mapcar (lambda (id) (gymnast-find-ir-node ir id))
      (gymnast-plan-node-field node 'inputs))))

(defun gymnast-node-dependency-slice (plan node)
  (mapcar
    (lambda (dependency)
      (let ((dep-node (gymnast-find-plan-node plan dependency)))
        (list dependency
          (if dep-node
            (gymnast-plan-node-field dep-node 'fingerprint)
            'missing))))
    (gymnast-plan-node-field node 'depends-on)))

(defun gymnast-node-role (node)
  (cond
    ((equal (gymnast-plan-node-field node 'class) 'generative)
      "Produce one candidate implementation for this closed node contract.")
    ((equal (gymnast-plan-node-field node 'class) 'verification)
      "Materialize the independent verifier projection. Do not inspect or trust generator rationale.")
    ((equal (gymnast-plan-node-field node 'class) 'structural)
      "Apply the named deterministic compiler recipe exactly.")
    (t "Assemble only the declared artifacts and capability edges.")))

(defun gymnast-target-content-hint (target)
  (let ((lang (if (consp target) (car target) target)))
    (cond
      ((equal lang 'ruby) "<valid Ruby source code>")
      ((equal lang 'go) "<valid Go source code>")
      ((equal lang 'java) "<valid Java source code>")
      ((equal lang 'python) "<valid Python source code>")
      (t "<complete-content>"))))

(defun gymnast-output-protocol (node)
  (let ((hint (gymnast-target-content-hint
          (gymnast-plan-node-field node 'target))))
    (list 'candidate
      (list 'schema $gymnast-candidate-schema)
      (list 'node-id (gymnast-plan-node-id node))
      (list 'files
        (mapcar (lambda (path) (list path hint))
          (gymnast-plan-node-field node 'may-write)))
      (list 'implements "<ir-node-id-list>")
      (list 'edge-uses nil)
      (list 'assumptions nil)
      (list 'unresolved nil))))

;;; Structured projection: capability contracts from platform kit.

(defun gymnast-find-capability-def (name caps)
  (if (null caps) nil
    (if (equal (gymnast-capability-field (car caps) 'name) name)
      (car caps)
      (gymnast-find-capability-def name (cdr caps)))))

(defun gymnast-format-capability (cap-name kit-caps)
  (let* ((nl (code-char 10))
      (cap-def (gymnast-find-capability-def cap-name kit-caps)))
    (if cap-def
      (concat "  " (gymnast-symbol-string cap-name)
        " (v" (gymnast-capability-field cap-def 'version) "):" nl
        "    Guarantees: "
        (gymnast-join-strings
          (mapcar #'gymnast-symbol-string
            (gymnast-capability-field cap-def 'guarantees))
          ", ") nl
        "    Failure modes: "
        (gymnast-join-strings
          (mapcar #'gymnast-symbol-string
            (gymnast-capability-field cap-def 'failure-modes))
          ", "))
      (concat "  " (gymnast-symbol-string cap-name)))))

(defun gymnast-project-capability-contracts (node)
  (let* ((nl (code-char 10))
      (caps (gymnast-plan-node-field node 'capabilities))
      (kit-caps (if (boundp '$gymnast-ruby-platform-capabilities)
          $gymnast-ruby-platform-capabilities nil)))
    (if (or (null caps) (equal caps '((none))))
      ""
      (concat "CAPABILITY CONTRACTS" nl
        (gymnast-join-strings
          (mapcar (lambda (cap-name)
              (gymnast-format-capability cap-name kit-caps))
            caps)
          nl) nl nl))))

;;; Structured projection: state model from state IR nodes.

(defun gymnast-format-state-node (node)
  (let* ((nl (code-char 10))
      (name (gymnast-ir-node-field node 'name))
      (fields (gymnast-ir-node-field node 'fields))
      (of-val (gymnast-assoc-value ':of fields))
      (durability (gymnast-assoc-value ':durability fields))
      (aggregate (gymnast-assoc-value ':aggregate fields))
      (versioned (gymnast-assoc-value ':versioned fields))
      (consistency (gymnast-assoc-value ':consistency fields)))
    (concat "  " (gymnast-symbol-string name) ":" nl
      (if aggregate
        (concat "    Aggregate: " (prin1-to-string aggregate) nl) "")
      (if versioned
        (concat "    Versioning: " (gymnast-symbol-string versioned) nl)
        "")
      (if consistency
        (concat "    Consistency: "
          (gymnast-symbol-string consistency) nl)
        "")
      (if durability
        (concat "    Durability: "
          (gymnast-symbol-string durability) nl)
        "")
      (if of-val
        (concat "    Entities: "
          (gymnast-join-strings
            (mapcar #'gymnast-symbol-string
              (if (and (consp of-val) (equal (car of-val) 'aggregate))
                (cdr of-val)
                (if (consp of-val) of-val (list of-val))))
            ", "))
        ""))))

(defun gymnast-project-state-model (ir-slice)
  (let* ((nl (code-char 10))
      (state-nodes (filter
          (lambda (node)
            (equal (gymnast-ir-node-kind node) 'state))
          ir-slice)))
    (if (null state-nodes) ""
      (concat "STATE MODEL" nl
        (gymnast-join-strings
          (mapcar #'gymnast-format-state-node state-nodes)
          nl) nl nl))))

;;; Structured projection: type reference from type IR nodes.

(defun gymnast-format-type-node (node)
  (let* ((nl (code-char 10))
      (name (gymnast-ir-node-field node 'name))
      (fields (gymnast-ir-node-field node 'fields))
      (opaque (gymnast-assoc-value ':opaque fields))
      (record (gymnast-assoc-value ':record fields))
      (enum (gymnast-assoc-value ':enum fields))
      (variant (gymnast-assoc-value ':variant fields)))
    (cond
      (opaque
        (concat "  " (gymnast-symbol-string name)
          " (opaque " (gymnast-symbol-string opaque) ")"))
      (enum
        (concat "  " (gymnast-symbol-string name)
          " (enum): "
          (gymnast-join-strings
            (mapcar #'gymnast-symbol-string enum) " | ")))
      (record
        (concat "  " (gymnast-symbol-string name) " (record):" nl
          (gymnast-join-strings
            (mapcar
              (lambda (field-def)
                (concat "    " (gymnast-symbol-string (car field-def))
                  ": " (prin1-to-string (cadr field-def))))
              record)
            nl)))
      (variant
        (concat "  " (gymnast-symbol-string name) " (variant): "
          (gymnast-join-strings
            (mapcar
              (lambda (v)
                (concat (gymnast-symbol-string (car v))
                  " " (gymnast-symbol-string (cadr v))))
              variant)
            " | ")))
      (t (concat "  " (gymnast-symbol-string name))))))

(defun gymnast-project-type-reference (ir-slice)
  (let* ((nl (code-char 10))
      (type-nodes (filter
          (lambda (node)
            (equal (gymnast-ir-node-kind node) 'type))
          ir-slice)))
    (if (null type-nodes) ""
      (concat "TYPE REFERENCE" nl
        (gymnast-join-strings
          (mapcar #'gymnast-format-type-node type-nodes) nl)
        nl nl))))

;;; Structured projection: behavioral reference from behavior IR nodes.

(defun gymnast-format-failure-clause (f)
  (let* ((error-name (car f))
      (when-pred (gymnast-keyword-value (cdr f) ':when))
      (preserves (gymnast-keyword-value (cdr f) ':preserves)))
    (concat "      " (gymnast-symbol-string error-name)
      (if when-pred
        (concat " when " (prin1-to-string when-pred)) "")
      (if preserves
        (concat ", preserves " (gymnast-symbol-string preserves)) ""))))

(defun gymnast-format-behavior-node (node)
  (let* ((nl (code-char 10))
      (name (gymnast-ir-node-field node 'name))
      (tr (gymnast-extract-transition node))
      (operation (gymnast-transition-field tr 'operation))
      (actor (gymnast-transition-field tr 'actor))
      (reads (gymnast-transition-field tr 'reads))
      (writes (gymnast-transition-field tr 'writes))
      (atomic (gymnast-transition-field tr 'atomic))
      (idempotency (gymnast-transition-field tr 'idempotency))
      (preconditions (gymnast-transition-field tr 'preconditions))
      (postconditions (gymnast-transition-field tr 'postconditions))
      (failures (gymnast-transition-field tr 'failures))
      (emissions (gymnast-transition-field tr 'emissions)))
    (concat "  " (gymnast-symbol-string name)
      (if operation (concat " (" operation ")") "") ":" nl
      (if actor
        (concat "    Actor: " (prin1-to-string actor) nl) "")
      (if reads
        (concat "    Reads: "
          (gymnast-join-strings
            (mapcar #'gymnast-symbol-string reads) ", ") nl)
        "")
      (if writes
        (concat "    Writes: "
          (gymnast-join-strings
            (mapcar #'gymnast-symbol-string writes) ", ") nl)
        "")
      (if atomic
        (concat "    Atomic scope: " (prin1-to-string atomic) nl) "")
      (if idempotency
        (concat "    Idempotency: "
          (gymnast-symbol-string idempotency) nl)
        "")
      (if preconditions
        (concat "    Preconditions:" nl
          (gymnast-join-strings
            (mapcar
              (lambda (p) (concat "      " (prin1-to-string (car p))))
              preconditions)
            nl) nl)
        "")
      (if postconditions
        (concat "    Postconditions:" nl
          (gymnast-join-strings
            (mapcar
              (lambda (p) (concat "      " (prin1-to-string (car p))))
              postconditions)
            nl) nl)
        "")
      (if failures
        (concat "    Failures:" nl
          (gymnast-join-strings
            (mapcar #'gymnast-format-failure-clause failures)
            nl) nl)
        "")
      (if emissions
        (concat "    Emissions:" nl
          (gymnast-join-strings
            (mapcar
              (lambda (e) (concat "      " (prin1-to-string e)))
              emissions)
            nl) nl)
        ""))))

(defun gymnast-project-behavioral-reference (ir-slice)
  (let* ((nl (code-char 10))
      (behavior-nodes (filter
          (lambda (node)
            (equal (gymnast-ir-node-kind node) 'behavior))
          ir-slice)))
    (if (null behavior-nodes) ""
      (concat "BEHAVIORAL REFERENCE" nl
        (gymnast-join-strings
          (mapcar #'gymnast-format-behavior-node behavior-nodes)
          nl) nl nl))))

;;; Target language formatting.

(defun gymnast-target-language-name (target)
  (if (consp target)
    (gymnast-symbol-string (car target))
    (if target (gymnast-symbol-string target) "unknown")))

(defun gymnast-target-framework-hint (target)
  (if (not (consp target)) ""
    (let ((lang (car target))
        (framework (gymnast-keyword-value (cdr target) ':framework)))
      (if (not framework) ""
        (let ((nl (code-char 10)))
          (cond
            ((and (equal lang 'ruby) (equal framework 'rails))
              (concat nl
                "Use Rails conventions: ActiveRecord models, "
                "ApplicationRecord base class, "
                "standard Rails error handling."))
            ((and (equal lang 'go) (equal framework 'stdlib))
              (concat nl
                "Use Go stdlib conventions: exported types with "
                "receiver methods, error returns, "
                "context.Context parameters."))
            ((and (equal lang 'java) (equal framework 'spring))
              (concat nl
                "Use Spring conventions: @Repository/@Service "
                "annotations, JPA entities, "
                "@Transactional methods."))
            ((and (equal lang 'python) (equal framework 'django))
              (concat nl
                "Use Django conventions: models.Model subclasses, "
                "Manager/QuerySet patterns, "
                "transaction.atomic blocks." nl
                "PYTHON STRING REQUIREMENT: use ONLY single-quoted "
                "strings ('...') everywhere in the Python code. "
                "Never use double-quoted strings or triple-double-quoted "
                "docstrings — use triple-single-quoted ('''...''') for "
                "docstrings instead. This is mandatory because the code "
                "is inside an S-expression double-quoted string."))
            (t (concat nl "Use "
                (gymnast-symbol-string framework)
                " conventions."))))))))

(defun gymnast-target-section (node)
  (let* ((nl (code-char 10))
      (target (gymnast-plan-node-field node 'target))
      (lang (gymnast-target-language-name target))
      (ext (gymnast-target-file-extension target))
      (paths (gymnast-plan-node-field node 'may-write)))
    (concat "TARGET" nl
      (prin1-to-string target) nl nl
      "CRITICAL: Every file content string in FILES must be valid "
      lang " source code." nl
      "The output files end in " ext " — their content is " lang
      ", never Lisp/Scheme/Clojure/pseudocode." nl
      "The S-expression envelope (candidate ...) is metadata framing; "
      "file content strings inside it are real " lang " source code." nl
      "ESCAPING: file content is inside S-expression double-quoted strings. "
      "Backslash-escape every literal double-quote and every literal "
      "backslash in file content. Prefer single-quoted strings in "
      lang " where the language allows it."
      (gymnast-target-framework-hint target)
      nl nl)))

;;; Prompt text assembly.

(defun gymnast-prompt-text (node ir-slice dependency-slice)
  (let ((nl (code-char 10)))
    (concat
      "GYMNAST NODE CONTRACT" nl
      "Node: " (gymnast-plan-node-id node) nl
      "Recipe: " (princ-to-string
        (gymnast-plan-node-field node 'recipe)) nl
      "Role: " (gymnast-node-role node) nl nl
      (gymnast-target-section node)
      (gymnast-project-capability-contracts node)
      (gymnast-project-state-model ir-slice)
      (gymnast-project-type-reference ir-slice)
      (gymnast-project-behavioral-reference ir-slice)
      "OBLIGATIONS" nl
      (gymnast-join-strings
        (mapcar (lambda (ob) (concat "  " (gymnast-symbol-string ob)))
          (gymnast-plan-node-field node 'obligations))
        nl) nl nl
      "PROHIBITIONS" nl
      (gymnast-join-strings
        (mapcar (lambda (p) (concat "  " (gymnast-symbol-string p)))
          (gymnast-plan-node-field node 'prohibitions))
        nl) nl nl
      "AUTHORIZED FILES" nl
      (prin1-to-string (gymnast-plan-node-field node 'may-write))
      nl nl
      "DEPENDENCIES" nl
      (prin1-to-string dependency-slice) nl nl
      "OUTPUT PROTOCOL" nl
      (prin1-to-string (gymnast-output-protocol node)) nl nl
      "AUTHORITATIVE INPUT (reference)" nl
      (prin1-to-string ir-slice) nl nl
      "Return only the candidate S-expression. Report no confidence "
      "score. If the contract is not locally closed, return an "
      "unresolved entry and no files.")))

(defun gymnast-compile-prompt (ir plan node)
  (let* ((ir-slice (gymnast-node-ir-slice ir node))
      (dependency-slice (gymnast-node-dependency-slice plan node))
      (text (gymnast-prompt-text node ir-slice dependency-slice))
      (base
        (list 'prompt-package
          (list 'schema $gymnast-prompt-schema)
          (list 'node-id (gymnast-plan-node-id node))
          (list 'node-fingerprint
            (gymnast-plan-node-field node 'fingerprint))
          (list 'executor (gymnast-plan-node-field node 'class))
          (list 'model-policy (gymnast-plan-node-field node 'model))
          (list 'ir-slice ir-slice)
          (list 'dependency-slice dependency-slice)
          (list 'output-protocol (gymnast-output-protocol node))
          (list 'text text))))
    (append base (list (list 'fingerprint (gymnast-fingerprint base))))))

(defun gymnast-compile-prompts (ir plan)
  (mapcar (lambda (node) (gymnast-compile-prompt ir plan node))
    (gymnast-plan-field plan 'nodes)))
