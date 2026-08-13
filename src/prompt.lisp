;;; Stable prompt compilation from node contracts.
;;;
;;; A prompt is a projection of an authoritative node contract.  Editing the
;;; prose cannot change the contract; changing the contract changes its
;;; fingerprint and therefore the prompt fingerprint.

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

(defun gymnast-output-protocol (node)
  (list 'candidate
    (list 'schema $gymnast-candidate-schema)
    (list 'node-id (gymnast-plan-node-id node))
    (list 'files
      (mapcar (lambda (path) (list path "<complete-content>"))
        (gymnast-plan-node-field node 'may-write)))
    (list 'implements "<ir-node-id-list>")
    (list 'edge-uses "<declared-capability-edge-list>")
    (list 'assumptions nil)
    (list 'unresolved nil)))

(defun gymnast-prompt-text (node ir-slice dependency-slice)
  (let ((newline (code-char 10)))
    (concat
      "GYMNAST NODE CONTRACT" newline
      "Node: " (gymnast-plan-node-id node) newline
      "Recipe: " (princ-to-string (gymnast-plan-node-field node 'recipe)) newline
      "Role: " (gymnast-node-role node) newline newline
      "AUTHORITATIVE INPUT" newline
      (prin1-to-string ir-slice) newline newline
      "DEPENDENCIES" newline
      (prin1-to-string dependency-slice) newline newline
      "TARGET" newline
      (prin1-to-string (gymnast-plan-node-field node 'target)) newline
      "IMPORTANT: All file content strings in the FILES list MUST be source code "
      "written in the language specified by the TARGET above, not Lisp, Scheme, "
      "Clojure, or pseudocode. The S-expression envelope wraps candidate metadata; "
      "each file content string is real source code in the target language. "
      "For TARGET (RUBY :FRAMEWORK RAILS), write Ruby. "
      "For TARGET (GO :FRAMEWORK STDLIB), write Go. "
      "For TARGET (JAVA :FRAMEWORK SPRING), write Java."
      newline newline
      "AUTHORIZED FILES" newline
      (prin1-to-string (gymnast-plan-node-field node 'may-write)) newline newline
      "CAPABILITIES" newline
      (prin1-to-string (gymnast-plan-node-field node 'capabilities)) newline newline
      "OBLIGATIONS" newline
      (prin1-to-string (gymnast-plan-node-field node 'obligations)) newline newline
      "PROHIBITIONS" newline
      (prin1-to-string (gymnast-plan-node-field node 'prohibitions)) newline newline
      "OUTPUT PROTOCOL" newline
      (prin1-to-string (gymnast-output-protocol node)) newline newline
      "Return only the candidate S-expression. Report no confidence score. "
      "If the contract is not locally closed, return an unresolved entry and no files.")))

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
