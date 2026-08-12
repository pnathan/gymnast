;;; Canonical serialization contract and trust-boundary validation.
;;;
;;; Canonical rules enforced by construction and validated at ingestion:
;;;   1. Association-list fields sorted lexicographically by key.
;;;   2. Identifier lists (inputs, depends-on, may-write) sorted.
;;;   3. Behavioral clause order preserved (sequence can be semantic).
;;;   4. Schema version present in every artifact root.
;;;   5. Fingerprint computed over canonical form excluding fingerprint field.
;;;
;;; The fingerprint function is FNV-1a (provisional).  Replacing it with
;;; SHA-256 changes only gymnast-fingerprint-string and the prefix tag.

(defun gymnast-fields-canonical-p (fields)
  (if (or (null fields) (null (cdr fields)))
    t
    (let ((key-a (car (car fields)))
        (key-b (car (cadr fields))))
      (if (not (and (symbolp key-a) (symbolp key-b)))
        nil
        (and (string-lessp (gymnast-symbol-string key-a)
            (gymnast-symbol-string key-b))
          (gymnast-fields-canonical-p (cdr fields)))))))

(defun gymnast-strings-canonical-p (xs)
  (if (or (null xs) (null (cdr xs)))
    t
    (and (string-lessp (car xs) (cadr xs))
      (gymnast-strings-canonical-p (cdr xs)))))

(defun gymnast-validate-ir-node-canonical (node)
  (if (not (gymnast-ir-node-p node))
    (list (gymnast-diagnostic 'error 'non-canonical-value
        "ir-node" "value is not a tagged ir-node" node))
    (let ((fields (gymnast-ir-node-field node 'fields))
        (id (gymnast-ir-node-id node)))
      (if (gymnast-fields-canonical-p fields)
        nil
        (list (gymnast-diagnostic 'error 'non-canonical-field-order id
            "IR node fields are not in canonical order" fields))))))

(defun gymnast-validate-plan-node-canonical (node)
  (if (not (gymnast-plan-node-p node))
    (list (gymnast-diagnostic 'error 'non-canonical-value
        "plan-node" "value is not a tagged node-contract" node))
    (let ((id (gymnast-plan-node-id node))
        (inputs (gymnast-plan-node-field node 'inputs))
        (depends (gymnast-plan-node-field node 'depends-on))
        (writes (gymnast-plan-node-field node 'may-write)))
      (append
        (if (gymnast-strings-canonical-p inputs) nil
          (list (gymnast-diagnostic 'error 'non-canonical-inputs id
              "inputs not sorted" inputs)))
        (if (gymnast-strings-canonical-p depends) nil
          (list (gymnast-diagnostic 'error 'non-canonical-depends id
              "depends-on not sorted" depends)))
        (if (gymnast-strings-canonical-p writes) nil
          (list (gymnast-diagnostic 'error 'non-canonical-writes id
              "may-write not sorted" writes)))))))

(defun gymnast-validate-ir-canonical (ir)
  (if (not (gymnast-tagged-p 'ir ir))
    (list (gymnast-diagnostic 'error 'non-canonical-value "ir"
        "value is not a tagged ir" ir))
    (let ((schema (gymnast-ir-field ir 'schema)))
      (append
        (if schema nil
          (list (gymnast-diagnostic 'error 'missing-schema "ir"
              "IR is missing schema version" nil)))
        (reduce #'append
          (mapcar #'gymnast-validate-ir-node-canonical
            (gymnast-ir-all-nodes ir))
          nil)))))

(defun gymnast-validate-plan-canonical (plan)
  (if (not (gymnast-tagged-p 'plan plan))
    (list (gymnast-diagnostic 'error 'non-canonical-value "plan"
        "value is not a tagged plan" plan))
    (let ((schema (gymnast-plan-field plan 'schema)))
      (append
        (if schema nil
          (list (gymnast-diagnostic 'error 'missing-schema "plan"
              "plan is missing schema version" nil)))
        (reduce #'append
          (mapcar #'gymnast-validate-plan-node-canonical
            (gymnast-plan-field plan 'nodes))
          nil)))))

(defun gymnast-assert-canonical-ir (ir)
  (let ((diagnostics (gymnast-validate-ir-canonical ir)))
    (if (gymnast-has-errors-p diagnostics)
      (error (concat "non-canonical IR rejected: "
          (prin1-to-string diagnostics)))
      ir)))

(defun gymnast-assert-canonical-plan (plan)
  (let ((diagnostics (gymnast-validate-plan-canonical plan)))
    (if (gymnast-has-errors-p diagnostics)
      (error (concat "non-canonical plan rejected: "
          (prin1-to-string diagnostics)))
      plan)))

(defun gymnast-canonical-serialize (value)
  (concat (prin1-to-string value) (code-char 10)))

(defun gymnast-verify-fingerprint (artifact tag)
  (let* ((stored (gymnast-assoc-value 'fingerprint (cdr artifact)))
      (without (filter
          (lambda (entry) (not (equal (car entry) 'fingerprint)))
          (cdr artifact)))
      (recomputed (gymnast-fingerprint (cons tag without))))
    (if (equal stored recomputed)
      nil
      (list (gymnast-diagnostic 'error 'fingerprint-mismatch
          (princ-to-string tag)
          "stored fingerprint does not match recomputed value"
          (list 'stored stored 'recomputed recomputed))))))
