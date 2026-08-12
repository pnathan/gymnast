;;; Surface-to-IR elaboration and closed-world diagnostics.

(def $gymnast-common-fields '(:id :doc :tags))

(defun gymnast-allowed-fields (kind)
  (append
    $gymnast-common-fields
    (cond
      ((equal kind 'module) '(:version :owner :exports))
      ((equal kind 'import) '(:version :names :authority :arguments))
      ((equal kind 'application) '(:modules :uses :default-acceptance))
      ((equal kind 'actor) '(:kind :identity))
      ((equal kind 'type) '(:opaque :record :enum :variant))
      ((equal kind 'component) '(:responsibility :provides :uses))
      ((equal kind 'interface) nil)
      ((equal kind 'state)
       '(:of :owner :durability :initial :aggregate :versioned
         :partitioned-by :consistency))
      ((equal kind 'flow) '(:from :to :kind :data :grant :deny))
      ((equal kind 'behavior)
       '(:on :reads :writes :atomic :idempotency :consistency))
      ((equal kind 'invariant) '(:scope :always))
      ((equal kind 'constraint)
       '(:class :scope :under :must :priority :status))
      ((equal kind 'synthesis)
       '(:target :platform :model :attempts :temperature :may-use :must-not))
      ((equal kind 'acceptance) '(:subject :extends))
      (t nil))))

(defun gymnast-allowed-clauses (kind)
  (cond
    ((equal kind 'interface) '(operation command query event))
    ((equal kind 'behavior) '(requires ensures returns fails emits))
    ((equal kind 'acceptance)
     '(model property scenario concurrency fault coverage execution))
    (t nil)))

(defun gymnast-required-fields (kind)
  (cond
    ((equal kind 'actor) '(:kind))
    ((equal kind 'component) '(:responsibility))
    ((equal kind 'state) '(:of :owner :durability))
    ((equal kind 'behavior) '(:on))
    ((equal kind 'constraint) '(:class))
    ((equal kind 'synthesis) '(:target :model))
    ((equal kind 'acceptance) '(:subject))
    (t nil)))

(defun gymnast-parse-operands-rec
  (kind subject operands fields clauses diagnostics)
  (cond
    ((null operands)
     (list 'parsed fields clauses diagnostics))
    ((gymnast-keyword-p (car operands))
     (let ((key (car operands)))
       (cond
         ((null (cdr operands))
          (gymnast-parse-operands-rec
            kind subject nil fields clauses
            (append diagnostics
                    (list (gymnast-diagnostic
                            'error 'missing-field-value subject
                            (concat "missing value for " (princ-to-string key)) key)))))
         ((not (member key (gymnast-allowed-fields kind)))
          (gymnast-parse-operands-rec
            kind subject (cdr (cdr operands)) fields clauses
            (append diagnostics
                    (list (gymnast-diagnostic
                            'error 'unknown-field subject
                            (concat "unknown " (gymnast-symbol-string kind)
                                    " field " (princ-to-string key)) key)))))
         ((assoc key fields)
          (gymnast-parse-operands-rec
            kind subject (cdr (cdr operands)) fields clauses
            (append diagnostics
                    (list (gymnast-diagnostic
                            'error 'duplicate-field subject
                            (concat "duplicate field " (princ-to-string key)) key)))))
         (t
          (gymnast-parse-operands-rec
            kind subject (cdr (cdr operands))
            (append fields (list (list key (cadr operands))))
            clauses diagnostics)))))
    ((consp (car operands))
     (let* ((clause (car operands))
            (head (car clause)))
       (if (and (symbolp head) (member head (gymnast-allowed-clauses kind)))
           (gymnast-parse-operands-rec
             kind subject (cdr operands) fields
             (append clauses (list clause)) diagnostics)
           (gymnast-parse-operands-rec
             kind subject (cdr operands) fields clauses
             (append diagnostics
                     (list (gymnast-diagnostic
                             'error 'unknown-clause subject
                             (concat "unknown " (gymnast-symbol-string kind)
                                     " clause " (princ-to-string head)) clause)))))))
    (t
     (gymnast-parse-operands-rec
       kind subject (cdr operands) fields clauses
       (append diagnostics
               (list (gymnast-diagnostic
                       'error 'unexpected-operand subject
                       "expected a keyword field or a clause" (car operands))))))))

(defun gymnast-parse-operands (kind subject operands)
  (gymnast-parse-operands-rec kind subject operands nil nil nil))

(defun gymnast-parsed-fields (parsed) (cadr parsed))
(defun gymnast-parsed-clauses (parsed) (caddr parsed))
(defun gymnast-parsed-diagnostics (parsed) (cadddr parsed))

(defun gymnast-missing-required-diagnostics (kind subject fields required)
  (if (null required)
      nil
      (append
        (if (assoc (car required) fields)
            nil
            (list (gymnast-diagnostic
                    'error 'missing-required-field subject
                    (concat "missing required field "
                            (princ-to-string (car required)))
                    (car required))))
        (gymnast-missing-required-diagnostics
          kind subject fields (cdr required)))))

(defun gymnast-type-shape-count (fields)
  (length
    (filter (lambda (key) (assoc key fields))
            '(:opaque :record :enum :variant))))

(defun gymnast-type-shape-diagnostics (kind subject fields)
  (if (not (equal kind 'type))
      nil
      (let ((count (gymnast-type-shape-count fields)))
        (cond
          ((= count 1) nil)
          ((= count 0)
           (list (gymnast-diagnostic
                   'error 'missing-type-shape subject
                   "type requires exactly one of :opaque, :record, :enum, or :variant"
                   nil)))
          (t
           (list (gymnast-diagnostic
                   'error 'ambiguous-type-shape subject
                   "type has more than one representation"
                   fields)))))))

(defun gymnast-unresolved-diagnostics (kind subject fields)
  (if (and (equal kind 'constraint)
           (equal (gymnast-assoc-value ':status fields) 'unresolved))
      (list (gymnast-diagnostic
              'error 'unresolved-decision subject
              "an unresolved decision affects synthesis"
              fields))
      nil))

(defun gymnast-node-id (module-name kind name fields)
  (let ((explicit (gymnast-assoc-value ':id fields)))
    (if explicit
        (gymnast-symbol-string explicit)
        (concat (gymnast-symbol-string module-name) "/"
                (gymnast-symbol-string kind) "/"
                (gymnast-symbol-string name)))))

(defun gymnast-elaborate-declaration (module-name surface)
  (if (not (gymnast-surface-p surface))
      (list nil
            (list (gymnast-diagnostic
                    'error 'invalid-surface module-name
                    (if (gymnast-invalid-surface-p surface)
                        (caddr surface)
                        "module child did not produce a declaration")
                    surface)))
      (let* ((kind (gymnast-surface-kind surface))
             (name (gymnast-surface-name surface))
             (subject (concat (gymnast-symbol-string module-name) "/"
                              (gymnast-symbol-string kind) "/"
                              (gymnast-symbol-string name)))
             (parsed (gymnast-parse-operands
                       kind subject (gymnast-surface-operands surface)))
             (fields (gymnast-parsed-fields parsed))
             (diagnostics
               (append
                 (gymnast-parsed-diagnostics parsed)
                 (gymnast-missing-required-diagnostics
                   kind subject fields (gymnast-required-fields kind))
                 (gymnast-type-shape-diagnostics kind subject fields)
                 (gymnast-unresolved-diagnostics kind subject fields)))
             (node (gymnast-ir-node
                     (gymnast-node-id module-name kind name fields)
                     kind name fields (gymnast-parsed-clauses parsed)
                     (gymnast-surface-mechanism surface))))
        (list node diagnostics))))

(defun gymnast-elaborate-children (module-name children nodes diagnostics)
  (if (null children)
      (list nodes diagnostics)
      (let* ((result (gymnast-elaborate-declaration module-name (car children)))
             (node (car result))
             (new-diagnostics (cadr result)))
        (gymnast-elaborate-children
          module-name (cdr children)
          (if node (append nodes (list node)) nodes)
          (append diagnostics new-diagnostics)))))

(defun gymnast-duplicate-id-diagnostics (nodes seen)
  (if (null nodes)
      nil
      (let ((id (gymnast-ir-node-id (car nodes))))
        (append
          (if (member id seen)
              (list (gymnast-diagnostic
                      'error 'duplicate-semantic-id id
                      "semantic identifiers must be unique" id))
              nil)
          (gymnast-duplicate-id-diagnostics (cdr nodes) (cons id seen))))))

(defun gymnast-partition-nodes (nodes kinds)
  (gymnast-sort-ir-nodes
    (filter (lambda (node) (member (gymnast-ir-node-kind node) kinds)) nodes)))

(defun gymnast-elaborate (surface)
  (if (or (not (gymnast-surface-p surface))
          (not (equal (gymnast-surface-kind surface) 'module)))
      (let ((diagnostics
              (list (gymnast-diagnostic
                      'error 'expected-module 'root
                      "the compilation root must be a module declaration"
                      surface))))
        (list 'ir
              (list 'schema $gymnast-ir-schema)
              (list 'diagnostics diagnostics)))
      (let* ((module-name (gymnast-surface-name surface))
             (module-parsed
               (gymnast-parse-operands
                 'module module-name (gymnast-surface-operands surface)))
             (children-result
               (gymnast-elaborate-children
                 module-name (gymnast-surface-children surface) nil nil))
             (nodes (car children-result))
             (diagnostics
               (append
                 (gymnast-parsed-diagnostics module-parsed)
                 (cadr children-result)
                 (gymnast-duplicate-id-diagnostics nodes nil)))
             (base
               (list 'ir
                     (list 'schema $gymnast-ir-schema)
                     (list 'module
                           (list 'name module-name)
                           (list 'fields
                                 (gymnast-canonical-fields
                                   (gymnast-parsed-fields module-parsed))))
                     (list 'design
                           (gymnast-partition-nodes
                             nodes '(import application actor type component
                                     interface state flow)))
                     (list 'transitions
                           (gymnast-partition-nodes nodes '(behavior)))
                     (list 'obligations
                           (gymnast-partition-nodes
                             nodes '(invariant constraint acceptance)))
                     (list 'synthesis
                           (gymnast-partition-nodes nodes '(synthesis)))
                     (list 'diagnostics diagnostics))))
        (append base (list (list 'fingerprint (gymnast-fingerprint base)))))))

(defun gymnast-assert-valid-ir (ir)
  (let ((diagnostics (gymnast-ir-field ir 'diagnostics)))
    (if (gymnast-has-errors-p diagnostics)
        (error (concat "elaboration failed: " (prin1-to-string diagnostics)))
        ir)))

