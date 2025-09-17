// ==UserScript==
// @name TODO: Write name
// @description TODO: Write description
// @version 1.0.0
// @author Emma Puls <emma.puls@gmail.com>
// ==/UserScript==

// Script config
const INCLUDE_LOGS = false;

// Add selectors here
// const loadingMessagesSelector = "p[data-qa]";

// Log utility function
const log = (message, ...optionalParams) =>
  INCLUDE_LOGS && console.log(`[Script name here] ${message}`, ...optionalParams);

/**
 * Helper function
 * Waits for an element to exist in the DOM.
 * @param {string} selector Selector for element
 * @returns {Promise<Element>} Promise that resolves when the element exists
 */
const waitForElementToExist = async (selector) => {
  log(`[waitForElementToExist] Waiting for element: ${selector}`);
  while (!document.querySelector(selector)) {
    log(`[waitForElementToExist] Element not found: ${selector}`);
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  log(`[waitForElementToExist] Element found: ${selector}`);
  return document.querySelector(selector);
};

/**
 * Helper function
 * Creates a MutationObserver to observe changes on the page
 * Starts observing the specified selector and calls the callback function when mutations are observed.
 */
const createMutationObserver = (selector, callback) => {
	log(`[createMutationObserver] Creating observer for: ${selector}`);
	const element = document.querySelector(selector);
	if (!element) {
		log(`[createMutationObserver] Element not found: ${selector}`);
		return;
	}

	const observer = new MutationObserver((mutations) => {
		log(`[MutationObserver] Mutation observed`, mutations);
		callback(mutations);
	});

	observer.observe(element, {
		childList: true,
		subtree: true,
	});
	log(`[createMutationObserver] Observer attached for: ${selector}`);
	return observer;
};

/**
 * Function that runs when page has loaded.
 */
const fnToRunWhenPageLoads = async () => {
};

/**
 * Function to start script that observes the page state
 */
const whenPageLoads = () => {
  log(`Initializing`);

  if (document.readyState !== "loading") {
    log(`Document already loaded`);
    fnToRunWhenPageLoads();
  } else {
    document.addEventListener("DOMContentLoaded", function () {
      log(`DOMContentLoaded event fired`);
      fnToRunWhenPageLoads();
    });
  }
};

// Fix function name and call
log(`Script loaded`);
whenPageLoads();