;;; Candidate protocol validation.  A model can propose data; it cannot mutate
;;; a plan node or decide whether its own output is acceptable.

(defun gymnast-candidate-field (candidate key)
  (gymnast-assoc-value key (cdr candidate)))

(defun gymnast-candidate-file-paths (candidate)
  (mapcar #'car (or (gymnast-candidate-field candidate 'files) nil)))

(defun gymnast-candidate-diagnostics (node candidate)
  (cond
    ((not (gymnast-tagged-p 'candidate candidate))
      (list (gymnast-diagnostic
          'error 'invalid-candidate (gymnast-plan-node-id node)
          "model output is not a candidate value" candidate)))
    (t
      (let* ((node-id (gymnast-plan-node-id node))
          (allowed (gymnast-plan-node-field node 'may-write))
          (paths (gymnast-candidate-file-paths candidate))
          (wrong-node
            (if (equal (gymnast-candidate-field candidate 'node-id) node-id)
              nil
              (list (gymnast-diagnostic
                  'error 'candidate-node-mismatch node-id
                  "candidate names a different plan node"
                  (gymnast-candidate-field candidate 'node-id)))))
          (bad-paths
            (mapcar
              (lambda (path)
                (gymnast-diagnostic
                  'error 'unauthorized-output-path node-id
                  "candidate writes outside its node contract" path))
              (filter (lambda (path) (not (member path allowed))) paths)))
          (missing-paths
            (mapcar
              (lambda (path)
                (gymnast-diagnostic
                  'error 'missing-output-file node-id
                  "candidate omitted a required artifact" path))
              (filter (lambda (path) (not (member path paths))) allowed)))
          (assumptions
            (if (null (gymnast-candidate-field candidate 'assumptions))
              nil
              (list (gymnast-diagnostic
                  'error 'candidate-added-assumptions node-id
                  "candidate may not add assumptions"
                  (gymnast-candidate-field candidate 'assumptions)))))
          (unresolved
            (if (null (gymnast-candidate-field candidate 'unresolved))
              nil
              (list (gymnast-diagnostic
                  'error 'candidate-unresolved node-id
                  "candidate reported an unresolved contract"
                  (gymnast-candidate-field candidate 'unresolved)))))
          (target-lang (gymnast-plan-node-field node 'target))
          (target-key (gymnast-target-language target-lang))
          (non-lisp-target (and target-key
              (not (equal target-key 'lamedh))
              (not (equal target-key 'lisp))
              (not (equal target-key 'scheme))))
          (target-violations
            (if non-lisp-target
              (let ((files (or (gymnast-candidate-field candidate 'files) nil)))
                (filter (lambda (d) d)
                  (mapcar
                    (lambda (file-entry)
                      (let ((content (cadr file-entry)))
                        (if (and (stringp content)
                            (or (gymnast-string-contains content "(defun ")
                              (gymnast-string-contains content "(defvar ")
                              (gymnast-string-contains content "(defmacro ")
                              (gymnast-string-contains content "(define ")
                              (gymnast-string-contains content "(lambda ")
                              (gymnast-string-contains content "(module ")
                              (gymnast-string-contains content "(setq ")
                              (gymnast-string-contains content "(let* ")))
                          (gymnast-diagnostic
                            'error 'target-language-violation node-id
                            (concat "file content appears to be Lisp, not "
                              (gymnast-symbol-string target-key)
                              " as required by TARGET")
                            (car file-entry))
                          nil)))
                    files)))
              nil))
          (declared-caps
            (gymnast-plan-node-field node 'capabilities))
          (edge-uses-raw
            (or (gymnast-candidate-field candidate 'edge-uses) nil))
          (edge-uses
            (if (and edge-uses-raw (listp edge-uses-raw))
              edge-uses-raw nil))
          (bad-edges
            (if edge-uses
              (mapcar
                (lambda (edge)
                  (gymnast-diagnostic
                    'error 'undeclared-edge-use node-id
                    (concat "candidate uses capability edge not "
                      "declared in node contract")
                    edge))
                (filter
                  (lambda (edge) (not (member edge declared-caps)))
                  edge-uses))
              nil)))
        (append wrong-node bad-paths missing-paths assumptions unresolved
          target-violations bad-edges)))))

(defun gymnast-candidate-valid-p (node candidate)
  (not (gymnast-has-errors-p
      (gymnast-candidate-diagnostics node candidate))))
