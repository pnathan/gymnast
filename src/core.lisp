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

(defun gymnast-string-contains (haystack needle)
  (let ((nlen (length needle)))
    (if (< (length haystack) nlen) nil
      (gymnast-string-contains-scan haystack needle nlen 0
        (- (length haystack) nlen)))))

(defun gymnast-string-contains-scan (haystack needle nlen idx limit)
  (if (> idx limit) nil
    (if (equal (substring haystack idx (+ idx nlen)) needle) t
      (gymnast-string-contains-scan haystack needle nlen (+ idx 1) limit))))

(defun gymnast-keyword-p (x)
  (and (symbolp x) (starts-with-p (princ-to-string x) ":")))

(defun gymnast-tagged-p (tag x)
  (and (consp x) (equal (car x) tag)))

;;; Canonical data helpers.

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

;;; FNV-1a fingerprinting.

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

;;; Record type declarations.

(defrecord gymnast-diagnostic severity code subject message details)

(defrecord gymnast-surface kind name operands children mechanism)

(defrecord gymnast-invalid-surface form message)

(defrecord gymnast-ir-node id kind name fields clauses surface-mechanism)

(defrecord gymnast-plan-node id class recipe inputs depends-on target model
  may-write capabilities obligations prohibitions fingerprint)

;;; Wrapper constructors preserve existing call-site API.

(defun gymnast-diagnostic (severity code subject message details)
  (make-gymnast-diagnostic severity code subject message details))

(defun gymnast-diagnostic-field (diagnostic key)
  (record-ref diagnostic key))

(defun gymnast-error-diagnostic-p (diagnostic)
  (and (gymnast-diagnostic-p diagnostic)
    (equal (gymnast-diagnostic-field diagnostic 'severity) 'error)))

(defun gymnast-has-errors-p (diagnostics)
  (gymnast-any #'gymnast-error-diagnostic-p diagnostics))

(defun gymnast-make-surface (kind name operands children mechanism)
  (make-gymnast-surface kind name operands children mechanism))

(defun gymnast-make-invalid-surface (form message)
  (make-gymnast-invalid-surface form message))

(defun gymnast-ir-node (id kind name fields clauses mechanism)
  (make-gymnast-ir-node id kind name
    (gymnast-canonical-fields fields) clauses mechanism))

(defun gymnast-ir-node-field (node key)
  (record-ref node key))

(defun gymnast-sort-ir-nodes (nodes)
  (sort nodes (lambda (a b)
      (string-lessp (gymnast-ir-node-id a)
        (gymnast-ir-node-id b)))))

;;; IR container accessors (IR stays as an alist).

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

;;; Plan node constructor.  Sorts collections, computes fingerprint from
;;; a canonical alist contract, then stores in a record.

(defun gymnast-plan-node (id class recipe inputs depends-on target model
    may-write capabilities obligations prohibitions)
  (let* ((sorted-inputs (sort inputs #'string-lessp))
      (sorted-depends (sort depends-on #'string-lessp))
      (sorted-write (sort may-write #'string-lessp))
      (sorted-caps (sort capabilities #'gymnast-canonical-less-p))
      (sorted-obs (sort obligations #'gymnast-canonical-less-p))
      (sorted-proh (sort prohibitions #'gymnast-canonical-less-p))
      (contract
        (list 'node-contract
          (list 'id id)
          (list 'class class)
          (list 'recipe recipe)
          (list 'inputs sorted-inputs)
          (list 'depends-on sorted-depends)
          (list 'target target)
          (list 'model model)
          (list 'may-write sorted-write)
          (list 'capabilities sorted-caps)
          (list 'obligations sorted-obs)
          (list 'prohibitions sorted-proh)))
      (fp (gymnast-fingerprint contract)))
    (make-gymnast-plan-node id class recipe sorted-inputs sorted-depends
      target model sorted-write sorted-caps sorted-obs sorted-proh fp)))

(defun gymnast-plan-node-field (node key)
  (record-ref node key))

;;; Plan container accessors (plan stays as an alist).

(defun gymnast-plan-field (plan key)
  (gymnast-assoc-value key (cdr plan)))

(defun gymnast-find-plan-node (plan id)
  (let ((matches
        (filter (lambda (node) (equal (gymnast-plan-node-id node) id))
          (gymnast-plan-field plan 'nodes))))
    (if matches (car matches) nil)))
