;;; Public compiler load unit.

(include "core.lisp")
(include "surface.lisp")
(include "elaborate.lisp")
(include "plan.lisp")
(include "prompt.lisp")
(include "candidate.lisp")

(defun gymnast-compile (surface)
  (let* ((ir (gymnast-elaborate surface))
         (plan (gymnast-plan ir))
         (prompts (gymnast-compile-prompts ir plan))
         (base
           (list 'compilation
                 (list 'schema "gymnast.compilation/0.1")
                 (list 'ir ir)
                 (list 'plan plan)
                 (list 'prompts prompts))))
    (append base (list (list 'fingerprint (gymnast-fingerprint base))))))

(defun gymnast-compilation-field (compilation key)
  (gymnast-assoc-value key (cdr compilation)))
