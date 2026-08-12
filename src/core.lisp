;;; Gymnast compiler core.
;;;
;;; This file contains only ordinary data constructors and pure-ish helpers.
;;; Surface reflection ends before values enter this layer.

(def $gymnast-ir-schema "gymnast.ir/0.1")
(def $gymnast-plan-schema "gymnast.plan/0.1")
(def $gymnast-prompt-schema "gymnast.prompt/0.1")
(def $gymnast-candidate-schema "gymnast.candidate/0.1")

(defun gymnast-assoc-value (key alist)
  (let ((entry (assoc key alist)))
    (if entry (cadr entry) nil)))

(defun gymnast-put-assoc (key value alist)
  (append
    (filter (lambda (entry) (not (equal (car entry) key))) alist)
    (list (list key value))))

(defun gymnast-all (predicate xs)
  (cond ((null xs) t)
        ((funcall predicate (car xs)) (gymnast-all predicate (cdr xs)))
        (t nil)))

(defun gymnast-any (predicate xs)
  (cond ((null xs) nil)
        ((funcall predicate (car xs)) t)
        (t (gymnast-any predicate (cdr xs)))))

(defun gymnast-unique (xs)
  (if (null xs)
      nil
      (cons (car xs)
            (gymnast-unique
              (filter (lambda (x) (not (equal x (car xs)))) (cdr xs))))))

(defun gymnast-symbol-string (x)
  (string-downcase (princ-to-string x)))

(defun gymnast-keyword-p (x)
  (and (symbolp x) (starts-with-p (princ-to-string x) ":")))

(defun gymnast-tagged-p (tag x)
  (and (consp x) (equal (car x) tag)))

(defun gymnast-diagnostic (severity code subject message details)
  (list 'diagnostic
        (list 'severity severity)
        (list 'code code)
        (list 'subject subject)
        (list 'message message)
        (list 'details details)))

(defun gymnast-diagnostic-field (diagnostic key)
  (gymnast-assoc-value key (cdr diagnostic)))

(defun gymnast-error-diagnostic-p (diagnostic)
  (and (gymnast-tagged-p 'diagnostic diagnostic)
       (equal (gymnast-diagnostic-field diagnostic 'severity) 'error)))

(defun gymnast-has-errors-p (diagnostics)
  (gymnast-any #'gymnast-error-diagnostic-p diagnostics))

;;; Surface declarations are intentionally small and untyped.  Elaboration
;;; validates and replaces them with closed IR nodes.

(defun gymnast-make-surface (kind name operands children mechanism)
  (list 'surface kind name operands children mechanism))

(defun gymnast-surface-p (x) (gymnast-tagged-p 'surface x))
(defun gymnast-surface-kind (x) (cadr x))
(defun gymnast-surface-name (x) (caddr x))
(defun gymnast-surface-operands (x) (cadddr x))
(defun gymnast-surface-children (x) (car (cdr (cdr (cdr (cdr x))))))
(defun gymnast-surface-mechanism (x) (car (cdr (cdr (cdr (cdr (cdr x)))))))

(defun gymnast-make-invalid-surface (form message)
  (list 'invalid-surface form message))

(defun gymnast-invalid-surface-p (x) (gymnast-tagged-p 'invalid-surface x))

;;; Canonical data helpers.  Fields and nodes are sorted; order inside a
;;; behavioral clause is preserved because sequence can be semantic.

(defun gymnast-canonical-less-p (a b)
  (string-lessp (prin1-to-string a) (prin1-to-string b)))

(defun gymnast-canonical-fields (fields)
  (sort fields (lambda (a b)
                 (string-lessp (gymnast-symbol-string (car a))
                               (gymnast-symbol-string (car b))))))

(defun gymnast-canonical-data (x)
  (cond
    ((null x) nil)
    ((atom x) x)
    (t (mapcar #'gymnast-canonical-data x))))

;;; FNV-1a is a deterministic first-cut fingerprint, not a security digest.
;;; It is deliberately named accordingly.  The artifact schema leaves room
;;; for a host-supplied SHA-256 implementation without changing IR identity.

(defun gymnast-fnv1a-step (hash chars)
  (if (null chars)
      hash
      (gymnast-fnv1a-step
        (* (logxor hash (char-code (car chars))) 1099511628211)
        (cdr chars))))

(defun gymnast-fingerprint-string (text)
  (clear-flag 'overflow)
  (let ((value (gymnast-fnv1a-step -3750763034362895579
                                    (string->list text))))
    (clear-flag 'overflow)
    (concat "fnv1a64:" (princ-to-string value))))

(defun gymnast-fingerprint (value)
  (gymnast-fingerprint-string (prin1-to-string value)))

(defun gymnast-ir-node (id kind name fields clauses mechanism)
  (list 'ir-node
        (list 'id id)
        (list 'kind kind)
        (list 'name name)
        (list 'fields (gymnast-canonical-fields fields))
        (list 'clauses clauses)
        (list 'surface-mechanism mechanism)))

(defun gymnast-ir-node-p (x) (gymnast-tagged-p 'ir-node x))
(defun gymnast-ir-node-field (node key)
  (gymnast-assoc-value key (cdr node)))
(defun gymnast-ir-node-id (node) (gymnast-ir-node-field node 'id))
(defun gymnast-ir-node-kind (node) (gymnast-ir-node-field node 'kind))

(defun gymnast-sort-ir-nodes (nodes)
  (sort nodes (lambda (a b)
                (string-lessp (gymnast-ir-node-id a)
                              (gymnast-ir-node-id b)))))

(defun gymnast-ir-field (ir key)
  (gymnast-assoc-value key (cdr ir)))

(defun gymnast-ir-all-nodes (ir)
  (append (gymnast-ir-field ir 'design)
          (gymnast-ir-field ir 'transitions)
          (gymnast-ir-field ir 'obligations)
          (gymnast-ir-field ir 'synthesis)))

(defun gymnast-ir-nodes-of-kind (ir kind)
  (filter (lambda (node) (equal (gymnast-ir-node-kind node) kind))
          (gymnast-ir-all-nodes ir)))

(defun gymnast-ir-node-ids (nodes)
  (mapcar #'gymnast-ir-node-id nodes))

(defun gymnast-find-ir-node (ir id)
  (let ((matches
          (filter (lambda (node) (equal (gymnast-ir-node-id node) id))
                  (gymnast-ir-all-nodes ir))))
    (if matches (car matches) nil)))

(defun gymnast-plan-node (id class recipe inputs depends-on target model
                             may-write capabilities obligations prohibitions)
  (let* ((contract
           (list 'node-contract
                 (list 'id id)
                 (list 'class class)
                 (list 'recipe recipe)
                 (list 'inputs (sort inputs #'string-lessp))
                 (list 'depends-on (sort depends-on #'string-lessp))
                 (list 'target target)
                 (list 'model model)
                 (list 'may-write (sort may-write #'string-lessp))
                 (list 'capabilities (sort capabilities #'gymnast-canonical-less-p))
                 (list 'obligations (sort obligations #'gymnast-canonical-less-p))
                 (list 'prohibitions (sort prohibitions #'gymnast-canonical-less-p))))
         (fingerprint (gymnast-fingerprint contract)))
    (append contract (list (list 'fingerprint fingerprint)))))

(defun gymnast-plan-node-p (x) (gymnast-tagged-p 'node-contract x))
(defun gymnast-plan-node-field (node key)
  (gymnast-assoc-value key (cdr node)))
(defun gymnast-plan-node-id (node) (gymnast-plan-node-field node 'id))

(defun gymnast-plan-field (plan key)
  (gymnast-assoc-value key (cdr plan)))

(defun gymnast-find-plan-node (plan id)
  (let ((matches
          (filter (lambda (node) (equal (gymnast-plan-node-id node) id))
                  (gymnast-plan-field plan 'nodes))))
    (if matches (car matches) nil)))

