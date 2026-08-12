;;; Sandboxed model runner with bounded repair.
;;;
;;; Generative plan nodes are executed by sending compiled prompt
;;; packages through a model-provider adapter.  The runner enforces
;;; the trust boundary: model output is parsed as data, never
;;; evaluated, and validated against the plan node contract before
;;; acceptance.  Failed validation feeds normalized diagnostics
;;; into a bounded repair loop.

;;; Model request preparation.

(defun gymnast-prepare-model-request (prompt-package)
  (list 'model-request
    (list 'node-id
      (gymnast-assoc-value 'node-id (cdr prompt-package)))
    (list 'prompt-text
      (gymnast-assoc-value 'text (cdr prompt-package)))
    (list 'model-policy
      (gymnast-assoc-value 'model-policy (cdr prompt-package)))
    (list 'prompt-fingerprint
      (gymnast-assoc-value 'fingerprint (cdr prompt-package)))))

(defun gymnast-model-request-field (request key)
  (gymnast-assoc-value key (cdr request)))

;;; Repair prompt generation.

(defun gymnast-repair-prompt (prompt-package diagnostics attempt)
  (let* ((nl (code-char 10))
      (original-text
        (gymnast-assoc-value 'text (cdr prompt-package)))
      (diag-text (gymnast-join-strings
          (mapcar (lambda (d)
              (concat "- "
                (princ-to-string
                  (gymnast-diagnostic-field d 'code))
                ": "
                (gymnast-diagnostic-field d 'message)))
            diagnostics)
          (code-char 10)))
      (repair-text (concat
          original-text nl nl
          "REPAIR ATTEMPT " (princ-to-string attempt) nl
          "The previous candidate was rejected. Fix these issues:" nl
          diag-text nl nl
          "Return only the corrected candidate S-expression.")))
    (gymnast-put-assoc 'text repair-text (cdr prompt-package))))

;;; Attempt provenance tracking.

(defun gymnast-make-attempt (number request response candidate
    diagnostics status)
  (list 'attempt
    (list 'number number)
    (list 'prompt-fingerprint
      (gymnast-model-request-field request 'prompt-fingerprint))
    (list 'response-length
      (if (stringp response) (length response) 0))
    (list 'diagnostics diagnostics)
    (list 'status status)))

(defun gymnast-attempt-field (attempt key)
  (gymnast-assoc-value key (cdr attempt)))

;;; Bounded execution loop.
;;;
;;; The runner calls provider-fn with a model-request and expects
;;; back a string containing an S-expression candidate.  It parses
;;; the response, validates the candidate, and retries on failure
;;; up to max-attempts times.
;;;
;;; provider-fn is a function (model-request) -> response-string.
;;; The runner never executes the response; it only parses it as data.

(defun gymnast-run-node-loop (ir plan node prompt-package
    provider-fn attempt max-attempts attempts)
  (if (> attempt max-attempts)
    (list 'run-result
      (list 'node-id (gymnast-plan-node-id node))
      (list 'status 'exhausted)
      (list 'attempts (reverse attempts))
      (list 'candidate nil))
    (let* ((request (gymnast-prepare-model-request prompt-package))
        (response (funcall provider-fn request))
        (candidate (if (stringp response)
            (gymnast-safe-read response)
            nil))
        (diagnostics (if candidate
            (gymnast-candidate-diagnostics node candidate)
            (list (gymnast-diagnostic 'error 'parse-failure
                (gymnast-plan-node-id node)
                "model response did not parse as a candidate"
                nil))))
        (valid (not (gymnast-has-errors-p diagnostics)))
        (this-attempt (gymnast-make-attempt
            attempt request response candidate diagnostics
            (if valid 'accepted 'rejected)))
        (new-attempts (cons this-attempt attempts)))
      (if valid
        (list 'run-result
          (list 'node-id (gymnast-plan-node-id node))
          (list 'status 'succeeded)
          (list 'attempts (reverse new-attempts))
          (list 'candidate candidate))
        (gymnast-run-node-loop ir plan node
          (cons 'prompt-package
            (gymnast-repair-prompt
              prompt-package diagnostics (+ attempt 1)))
          provider-fn (+ attempt 1) max-attempts new-attempts)))))

(defun gymnast-run-node (ir plan node provider-fn max-attempts)
  (let ((prompt-package (gymnast-compile-prompt ir plan node)))
    (gymnast-run-node-loop ir plan node prompt-package
      provider-fn 1 max-attempts nil)))

(defun gymnast-run-result-field (result key)
  (gymnast-assoc-value key (cdr result)))

;;; Safe reader: parse S-expression data without evaluation.
;;; In Lamedh, read-from-string returns parsed data without
;;; executing it, which is the correct trust boundary.
;;; Models sometimes wrap output in markdown fences despite
;;; the system prompt; extract the outermost S-expression by
;;; finding the first ( and last ) in the response.

(defun gymnast-find-char (chars ch idx)
  (cond ((null chars) nil)
    ((equal (car chars) ch) idx)
    (t (gymnast-find-char (cdr chars) ch (+ idx 1)))))

(defun gymnast-rfind-char (chars ch idx last)
  (cond ((null chars) last)
    ((equal (car chars) ch)
      (gymnast-rfind-char (cdr chars) ch (+ idx 1) idx))
    (t (gymnast-rfind-char (cdr chars) ch (+ idx 1) last))))

(defun gymnast-extract-sexpr (text)
  (let* ((chars (string->list text))
      (open-paren (code-char 40))
      (close-paren (code-char 41))
      (start (gymnast-find-char chars open-paren 0))
      (end (gymnast-rfind-char chars close-paren 0 nil)))
    (if (and start end (>= end start))
      (substring text start (+ end 1))
      text)))

(defun gymnast-safe-read (text)
  (if (or (null text) (equal text ""))
    nil
    (ignore-errors (read-from-string (gymnast-extract-sexpr text)))))

;;; Run all generative nodes in a plan.

(defun gymnast-run-generative-nodes (ir plan provider-fn max-attempts)
  (let* ((nodes (gymnast-plan-field plan 'nodes))
      (generative (filter
          (lambda (node)
            (equal (gymnast-plan-node-field node 'class) 'generative))
          nodes)))
    (mapcar
      (lambda (node)
        (gymnast-run-node ir plan node provider-fn max-attempts))
      generative)))

;;; Claude subprocess synthesizer.
;;;
;;; Invokes the Claude CLI as a subprocess to generate candidates.
;;; The model policy from the plan node is mapped to a --model flag.
;;; Prompt text is written to a temp file to avoid shell quoting issues.

(defun gymnast-claude-model-flag (model-policy)
  (cond
    ((and (consp model-policy)
        (equal (car model-policy) 'small-code-model))
      "haiku")
    ((and (consp model-policy) (symbolp (car model-policy)))
      (gymnast-symbol-string (car model-policy)))
    ((symbolp model-policy)
      (gymnast-symbol-string model-policy))
    ((stringp model-policy) model-policy)
    (t "haiku")))

(def $gymnast-claude-system-prompt
  (concat
    "You are a synthesis engine. "
    "Output only a single S-expression candidate value. "
    "No markdown fences, no explanation, no commentary."))

(defun gymnast-claude-subprocess-call (prompt-file model-flag)
  (shell (concat
      "claude -p"
      " --model " model-flag
      " --system-prompt \""
      $gymnast-claude-system-prompt
      "\" < " prompt-file)))

(defun gymnast-claude-subprocess-provider (model-request)
  (let* ((prompt-text
        (gymnast-model-request-field model-request 'prompt-text))
      (model-policy
        (gymnast-model-request-field model-request 'model-policy))
      (model-flag (gymnast-claude-model-flag model-policy))
      (node-id
        (gymnast-model-request-field model-request 'node-id))
      (prompt-file (concat "/tmp/gymnast-prompt-"
          (gymnast-fingerprint-string node-id) ".txt")))
    (write-file prompt-file prompt-text)
    (let* ((result (gymnast-claude-subprocess-call
            prompt-file model-flag))
        (exit-code (car result))
        (stdout (cadr result))
        (stderr (caddr result)))
      (delete-file prompt-file)
      (if (= exit-code 0)
        stdout
        nil))))

(defprotocol gymnast-synthesize)

(defrecord gymnast-claude-subprocess-synthesizer name)

(def $gymnast-claude-synthesizer
  (make-gymnast-claude-subprocess-synthesizer "claude-subprocess"))

(definstance gymnast-synthesize
  ((synth gymnast-claude-subprocess-synthesizer) model-request)
  string
  (gymnast-claude-subprocess-provider model-request))

(defun gymnast-make-claude-provider ()
  (lambda (model-request)
    (gymnast-synthesize $gymnast-claude-synthesizer model-request)))

;;; Convenience: run all generative nodes with the Claude subprocess.

(defun gymnast-run-with-claude (ir plan max-attempts)
  (gymnast-run-generative-nodes ir plan
    (gymnast-make-claude-provider) max-attempts))
