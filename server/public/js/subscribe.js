/**
 * Newsletter subscription module
 * Handles email subscription forms across the site
 */
(function() {
    'use strict';

    function initSubscribeForm(form) {
        if (!form) return;

        form.addEventListener('submit', async function(e) {
            e.preventDefault();

            const emailInput = form.querySelector('input[type="email"]');
            const submitBtn = form.querySelector('button[type="submit"]');

            if (!emailInput || !submitBtn) return;

            const email = emailInput.value.trim();
            if (!email) return;

            // Save original button text
            const originalText = submitBtn.textContent;

            // Disable form and show loading state
            emailInput.disabled = true;
            submitBtn.disabled = true;
            submitBtn.textContent = 'Subscribing...';

            try {
                const res = await fetch('/subscribe', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ email })
                });

                const data = await res.json();

                if (res.ok && data.success) {
                    // Success: Replace form with confirmation message
                    const container = form.parentElement;
                    const successMsg = document.createElement('p');
                    successMsg.className = 'subscribe-success';
                    successMsg.style.cssText = 'color: var(--cp-success, #22c55e); font-weight: 500; padding: 12px 0;';
                    successMsg.textContent = 'Thanks for subscribing! Check your inbox to confirm.';

                    form.style.display = 'none';
                    container.appendChild(successMsg);
                } else {
                    // Error: Show message and restore button
                    showError(form, data.error || 'Subscription failed. Please try again.');
                    restoreForm(emailInput, submitBtn, originalText);
                }
            } catch (err) {
                // Network error
                showError(form, 'Network error. Please try again.');
                restoreForm(emailInput, submitBtn, originalText);
            }
        });
    }

    function showError(form, message) {
        // Remove existing error if any
        const existing = form.querySelector('.subscribe-error');
        if (existing) existing.remove();

        const errorEl = document.createElement('p');
        errorEl.className = 'subscribe-error';
        errorEl.style.cssText = 'color: var(--cp-error, #ef4444); font-size: 0.875rem; margin-top: 8px;';
        errorEl.textContent = message;
        form.appendChild(errorEl);

        // Remove error after 3 seconds
        setTimeout(() => errorEl.remove(), 3000);
    }

    function restoreForm(emailInput, submitBtn, originalText) {
        emailInput.disabled = false;
        submitBtn.disabled = false;
        submitBtn.textContent = originalText;
    }

    // Initialize on DOM ready
    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', function() {
            initSubscribeForm(document.getElementById('emailForm'));
        });
    } else {
        initSubscribeForm(document.getElementById('emailForm'));
    }
})();
